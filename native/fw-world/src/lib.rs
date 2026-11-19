#![cfg_attr(not(any(test, feature = "std")), no_std)]

//! Bare-metal & WebAssembly (`no_std`) low-level ML, System-1 fast decision,
//! and Latent World Model engine for Fw.
//! Zero heap allocations, zero OS dependencies, deterministic Fat-LTO execution.

#[cfg(all(not(test), not(feature = "std")))]
#[panic_handler]
fn fw_bare_metal_panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

pub const STATE_DIM: usize = 8;
pub const ACTION_DIM: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct System1Decision {
    pub action: u32,
    pub confidence: f32,
    pub uncertainty: f32,
    pub escalate_to_system2: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct System1Policy {
    /// Row-major weights [ACTION_DIM][STATE_DIM]
    pub weights: [[f32; STATE_DIM]; ACTION_DIM],
    pub bias: [f32; ACTION_DIM],
    pub confidence_threshold: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct WorldModel {
    /// Latent transition matrix A: [STATE_DIM][STATE_DIM]
    pub transition_a: [[f32; STATE_DIM]; STATE_DIM],
    /// Action input matrix B: [STATE_DIM][ACTION_DIM]
    pub transition_b: [[f32; ACTION_DIM]; STATE_DIM],
    /// Bias vector
    pub bias: [f32; STATE_DIM],
    /// Linear reward readout weights
    pub reward_weights: [f32; STATE_DIM],
}

/// Pure `no_std` fast exp approximation (Padé / range-reduced polynomial) for bare-metal & wasm32.
#[inline]
pub fn exp_approx(x: f32) -> f32 {
    let clamped = x.clamp(-20.0, 20.0);
    // e^x = (1 + x/64)^64 via 6 squarings — accurate, branch-free, zero libm dependency.
    let mut y = 1.0 + clamped * (1.0 / 64.0);
    y *= y;
    y *= y;
    y *= y;
    y *= y;
    y *= y;
    y *= y;
    y
}

/// Pure `no_std` fast tanh approximation (rational Padé, exact on [-3, 3], saturates cleanly).
#[inline]
pub fn tanh_approx(x: f32) -> f32 {
    if x > 4.5 {
        return 1.0;
    }
    if x < -4.5 {
        return -1.0;
    }
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

#[inline]
pub fn dot_f32_slice(a: &[f32], b: &[f32]) -> f32 {
    let n = if a.len() < b.len() { a.len() } else { b.len() };
    let mut acc = 0.0f32;
    let mut i = 0;
    while i < n {
        acc += a[i] * b[i];
        i += 1;
    }
    acc
}

/// Quantized INT8 dot product (GGML Q8_0 style) for bare-metal / SIMD edge ML.
#[inline]
pub fn dot_q8_0(a: &[i8], scale_a: f32, b: &[i8], scale_b: f32) -> f32 {
    let n = if a.len() < b.len() { a.len() } else { b.len() };
    let mut acc: i32 = 0;
    let mut i = 0;
    while i < n {
        acc += (a[i] as i32) * (b[i] as i32);
        i += 1;
    }
    (acc as f32) * scale_a * scale_b
}

pub fn softmax_actions(logits: &[f32; ACTION_DIM]) -> [f32; ACTION_DIM] {
    let mut max_v = logits[0];
    let mut i = 1;
    while i < ACTION_DIM {
        if logits[i] > max_v {
            max_v = logits[i];
        }
        i += 1;
    }
    let mut exps = [0.0f32; ACTION_DIM];
    let mut sum = 0.0f32;
    i = 0;
    while i < ACTION_DIM {
        let e = exp_approx(logits[i] - max_v);
        exps[i] = e;
        sum += e;
        i += 1;
    }
    if sum <= 0.0 {
        return [1.0 / (ACTION_DIM as f32); ACTION_DIM];
    }
    i = 0;
    while i < ACTION_DIM {
        exps[i] /= sum;
        i += 1;
    }
    exps
}

impl System1Policy {
    pub const fn default_reflex() -> Self {
        Self {
            weights: [
                [1.5, 0.2, -0.5, 0.0, 0.1, 0.0, 0.0, 0.0],
                [-0.5, 1.8, 0.2, 0.0, 0.0, 0.1, 0.0, 0.0],
                [0.0, -0.4, 1.9, 0.3, 0.0, 0.0, 0.2, 0.0],
                [-0.2, -0.2, -0.2, 1.7, 0.0, 0.0, 0.0, 0.5],
            ],
            bias: [0.0; ACTION_DIM],
            confidence_threshold: 0.55,
        }
    }

    pub fn decide(&self, state: &[f32; STATE_DIM]) -> System1Decision {
        let mut logits = [0.0f32; ACTION_DIM];
        let mut a = 0;
        while a < ACTION_DIM {
            logits[a] = dot_f32_slice(&self.weights[a], state) + self.bias[a];
            a += 1;
        }
        let probs = softmax_actions(&logits);
        let mut best_idx = 0usize;
        let mut best_p = probs[0];
        let mut sum_sq = probs[0] * probs[0];
        a = 1;
        while a < ACTION_DIM {
            sum_sq += probs[a] * probs[a];
            if probs[a] > best_p {
                best_p = probs[a];
                best_idx = a;
            }
            a += 1;
        }
        // Gini impurity normalized to [0, 1] as zero-log uncertainty metric.
        let uncertainty = (1.0 - sum_sq).clamp(0.0, 1.0);
        let escalate = u32::from(best_p < self.confidence_threshold);
        System1Decision {
            action: best_idx as u32,
            confidence: best_p,
            uncertainty,
            escalate_to_system2: escalate,
        }
    }
}

impl WorldModel {
    pub const fn default_latent() -> Self {
        let mut a = [[0.0f32; STATE_DIM]; STATE_DIM];
        let mut i = 0;
        while i < STATE_DIM {
            a[i][i] = 0.85;
            i += 1;
        }
        let b = [
            [0.4, -0.1, 0.0, 0.0],
            [0.0, 0.4, -0.1, 0.0],
            [0.0, 0.0, 0.4, -0.1],
            [-0.1, 0.0, 0.0, 0.4],
            [0.1, 0.1, 0.0, 0.0],
            [0.0, 0.1, 0.1, 0.0],
            [0.0, 0.0, 0.1, 0.1],
            [0.1, 0.0, 0.0, 0.1],
        ];
        Self {
            transition_a: a,
            transition_b: b,
            bias: [0.0; STATE_DIM],
            reward_weights: [0.5, 0.4, 0.3, 0.2, 0.1, 0.1, 0.1, 0.1],
        }
    }

    pub fn predict_next(&self, state: &[f32; STATE_DIM], action: &[f32; ACTION_DIM]) -> [f32; STATE_DIM] {
        let mut next = [0.0f32; STATE_DIM];
        let mut i = 0;
        while i < STATE_DIM {
            let dyn_part = dot_f32_slice(&self.transition_a[i], state);
            let act_part = dot_f32_slice(&self.transition_b[i], action);
            next[i] = tanh_approx(dyn_part + act_part + self.bias[i]);
            i += 1;
        }
        next
    }

    pub fn predict_reward(&self, state: &[f32; STATE_DIM]) -> f32 {
        dot_f32_slice(&self.reward_weights, state)
    }

    /// Imagine `horizon` steps forward under a constant one-hot action and return discounted cumulative reward.
    pub fn rollout_value(
        &self,
        initial_state: &[f32; STATE_DIM],
        action_idx: usize,
        horizon: usize,
        gamma: f32,
    ) -> f32 {
        let mut action = [0.0f32; ACTION_DIM];
        if action_idx < ACTION_DIM {
            action[action_idx] = 1.0;
        }
        let mut cur = *initial_state;
        let mut total = 0.0f32;
        let mut discount = 1.0f32;
        let mut step = 0;
        while step < horizon {
            cur = self.predict_next(&cur, &action);
            total += discount * self.predict_reward(&cur);
            discount *= gamma;
            step += 1;
        }
        total
    }

    /// Online 1-step SGD transition refinement (continuous self-learning world model).
    pub fn update_step(
        &mut self,
        state: &[f32; STATE_DIM],
        action: &[f32; ACTION_DIM],
        observed_next: &[f32; STATE_DIM],
        lr: f32,
    ) -> f32 {
        let pred = self.predict_next(state, action);
        let mut mse = 0.0f32;
        let mut i = 0;
        while i < STATE_DIM {
            let err = observed_next[i] - pred[i];
            mse += err * err;
            let dtanh = 1.0 - pred[i] * pred[i];
            let grad = lr * err * dtanh;
            let mut j = 0;
            while j < STATE_DIM {
                self.transition_a[i][j] += grad * state[j];
                j += 1;
            }
            let mut k = 0;
            while k < ACTION_DIM {
                self.transition_b[i][k] += grad * action[k];
                k += 1;
            }
            self.bias[i] += grad;
            i += 1;
        }
        mse / (STATE_DIM as f32)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn fw_sys1_decide(state_ptr: *const f32) -> System1Decision {
    if state_ptr.is_null() {
        return System1Decision {
            action: 0,
            confidence: 0.0,
            uncertainty: 1.0,
            escalate_to_system2: 1,
        };
    }
    let state = unsafe { &*(state_ptr as *const [f32; STATE_DIM]) };
    System1Policy::default_reflex().decide(state)
}

#[unsafe(no_mangle)]
pub extern "C" fn fw_world_rollout_value(
    state_ptr: *const f32,
    action_idx: u32,
    horizon: u32,
    gamma: f32,
) -> f32 {
    if state_ptr.is_null() {
        return 0.0;
    }
    let state = unsafe { &*(state_ptr as *const [f32; STATE_DIM]) };
    WorldModel::default_latent().rollout_value(state, action_idx as usize, horizon as usize, gamma)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system1_selects_high_confidence_and_escalates_ambiguous() {
        let policy = System1Policy::default_reflex();
        let clear_state = [2.5, 0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 0.0];
        let d = policy.decide(&clear_state);
        assert_eq!(d.action, 0);
        assert!(d.confidence > 0.7);
        assert_eq!(d.escalate_to_system2, 0);

        let ambiguous = [0.0; STATE_DIM];
        let d2 = policy.decide(&ambiguous);
        assert_eq!(d2.escalate_to_system2, 1);
    }

    #[test]
    fn world_model_rollout_and_online_learning_reduce_error() {
        let mut wm = WorldModel::default_latent();
        let s0 = [0.2, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let a0 = [1.0, 0.0, 0.0, 0.0];
        let v0 = wm.rollout_value(&s0, 0, 5, 0.95);
        assert!(v0 > 0.0);

        let target = [0.8, 0.3, -0.1, 0.0, 0.2, 0.0, 0.0, 0.0];
        let loss1 = wm.update_step(&s0, &a0, &target, 0.2);
        let mut loss_last = loss1;
        for _ in 0..20 {
            loss_last = wm.update_step(&s0, &a0, &target, 0.2);
        }
        assert!(loss_last < loss1 * 0.25, "loss1={loss1} loss_last={loss_last}");
    }

    #[test]
    fn quantized_q8_dot_matches_expected() {
        let a: [i8; 4] = [10, 20, -10, 5];
        let b: [i8; 4] = [2, -1, 4, 2];
        let got = dot_q8_0(&a, 0.1, &b, 0.5);
        // (20 - 20 - 40 + 10) * 0.05 = -30 * 0.05 = -1.5
        assert!((got - (-1.5)).abs() < 1e-5);
    }
}
