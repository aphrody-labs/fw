//! Pure zero-I/O, zero-clock, zero-allocation request recovery state machine.
//! Exact 1-to-1 Rust port of `vercel-labs/fx/src/core/recovery/recovery.zig`.
//!
//! Only `Event::Succeeded` finishes a request successfully, and only
//! `Event::UserCancel` finishes it any other way (`NeverGivesUp`, `Heals`).

use serde::{Deserialize, Serialize};

/// Waits after dropped, server, and rate-limit failures without `Retry-After`.
pub const BACKOFF_MS: [u32; 6] = [1_000, 2_000, 4_000, 8_000, 16_000, 30_000];
/// Waits while offline (connect failures cost nothing billable).
pub const OFFLINE_MS: [u32; 3] = [1_000, 2_000, 5_000];
/// Jitter floor percentage (80..=100% of table value).
pub const JITTER_FLOOR_PERCENT: u32 = 80;
/// How long a resend keeps its retry status before handing off to thinking row.
pub const HANDOFF_MS: u64 = 2_000;
/// Consecutive same-kind failures before escalating (e.g., dropping Fast).
pub const ESCALATE_AFTER: u8 = 3;
/// Abrupt in-flight deaths before prompting the user once.
pub const CRASH_LOOP_AFTER: u8 = 2;
/// How long a wait for sign-in lasts before polling automatically.
pub const SIGN_IN_POLL_MS: u64 = 180_000;
/// Record binary wire format version.
pub const RECORD_VERSION: u8 = 1;
/// Max short status columns (48-col row minus 2-col status glyph).
pub const SHORT_COLUMNS: usize = 46;

/// Opaque 32-byte saved state persisted across crashes / restarts.
pub type Record = [u8; 32];

/// Fixed-capacity stack string matching `recovery.zig` `Text` (no heap alloc).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Text {
    pub len: u8,
    pub bytes: [u8; Self::CAPACITY],
}

impl Text {
    pub const CAPACITY: usize = 95;

    pub const fn empty() -> Self {
        Self {
            len: 0,
            bytes: [0u8; Self::CAPACITY],
        }
    }

    pub fn from_str_trunc(s: &str) -> Self {
        let mut out = Self::empty();
        let src = s.as_bytes();
        let n = src.len().min(Self::CAPACITY);
        out.bytes[..n].copy_from_slice(&src[..n]);
        out.len = n as u8;
        out
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

impl Default for Text {
    fn default() -> Self {
        Self::empty()
    }
}

impl core::fmt::Debug for Text {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Offline = 1,
    Dropped = 2,
    Server = 3,
    RateLimited = 4,
    TooLong = 5,
    Rejected = 6,
    SignIn = 7,
    Quota = 8,
    ContentFilter = 9,
    Unreadable = 10,
    Refused = 11,
}

impl FailureKind {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Offline),
            2 => Some(Self::Dropped),
            3 => Some(Self::Server),
            4 => Some(Self::RateLimited),
            5 => Some(Self::TooLong),
            6 => Some(Self::Rejected),
            7 => Some(Self::SignIn),
            8 => Some(Self::Quota),
            9 => Some(Self::ContentFilter),
            10 => Some(Self::Unreadable),
            11 => Some(Self::Refused),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Offline => "Offline — waiting to reconnect",
            Self::Dropped => "Connection lost — retrying",
            Self::Server => "Provider error — retrying",
            Self::RateLimited => "Rate limited — waiting",
            Self::TooLong => "Context too long",
            Self::Rejected => "Request rejected by provider",
            Self::SignIn => "Sign-in required",
            Self::Quota => "Quota exceeded",
            Self::ContentFilter => "Blocked by content filter",
            Self::Unreadable => "Unreadable stream — retrying",
            Self::Refused => "Gateway refused request — retrying",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub retry_after_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Start {
    pub resume_from: Option<Record>,
    pub fast: bool,
    pub stale: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Start(Start),
    Responding,
    Succeeded,
    Failed(Failure),
    Tick,
    UserContinue,
    SignInClosed,
    UserCancel,
    Closing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SendPlan {
    pub compact_first: bool,
    pub no_fast: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finish {
    Succeeded,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusKind {
    Clear,
    Waiting,
    Retrying,
    NeedUser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    SignIn = 1,
    Quota = 2,
    ContentFilter = 3,
    Rejected = 4,
    TooLong = 5,
    CrashLoop = 6,
    Stale = 7,
}

impl Need {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::SignIn),
            2 => Some(Self::Quota),
            3 => Some(Self::ContentFilter),
            4 => Some(Self::Rejected),
            5 => Some(Self::TooLong),
            6 => Some(Self::CrashLoop),
            7 => Some(Self::Stale),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub kind: StatusKind,
    pub need: Option<Need>,
    pub text: Text,
    pub short: Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Plan {
    pub save: Option<Record>,
    pub send: Option<SendPlan>,
    pub status: Option<Status>,
    pub finish: Option<Finish>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
enum Phase {
    #[default]
    Idle = 0,
    Sending = 1,
    Waiting = 2,
    NeedUser = 3,
}

impl Phase {
    const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Idle),
            1 => Some(Self::Sending),
            2 => Some(Self::Waiting),
            3 => Some(Self::NeedUser),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct State {
    phase: Phase,
    clean_exit: bool,
    no_fast: bool,
    compacted: bool,
    need: Option<Need>,
    pub attempt: u16,
    pub cause: Option<FailureKind>,
    pub streak: u8,
    streak_kind: Option<FailureKind>,
    backoff_step: u8,
    offline_step: u8,
    abrupt_deaths: u8,
    seed: u32,
    carried_ms: u32,
    started_ms: u64,
    wait_until_ms: u64,
    fast: bool,
    flowing: bool,
    sent_ms: u64,
    asked_ms: u64,
    polling: bool,
    continue_pending: bool,
}

impl State {
    pub const fn new() -> Self {
        Self {
            phase: Phase::Idle,
            clean_exit: false,
            no_fast: false,
            compacted: false,
            need: None,
            attempt: 0,
            cause: None,
            streak: 0,
            streak_kind: None,
            backoff_step: 0,
            offline_step: 0,
            abrupt_deaths: 0,
            seed: 1,
            carried_ms: 0,
            started_ms: 0,
            wait_until_ms: 0,
            fast: false,
            flowing: false,
            sent_ms: 0,
            asked_ms: 0,
            polling: false,
            continue_pending: false,
        }
    }

    pub fn encode(&self) -> Record {
        let mut r = [0u8; 32];
        r[0] = RECORD_VERSION;
        r[1] = self.phase as u8;
        let flags = (self.clean_exit as u8)
            | ((self.no_fast as u8) << 1)
            | ((self.compacted as u8) << 2);
        r[2] = flags;
        r[3] = self.need.map_or(0, |n| n as u8);
        r[4..6].copy_from_slice(&self.attempt.to_le_bytes());
        r[6] = self.cause.map_or(0, |c| c as u8);
        r[7] = self.streak;
        r[8] = self.streak_kind.map_or(0, |c| c as u8);
        r[9] = self.backoff_step;
        r[10] = self.offline_step;
        r[11] = self.abrupt_deaths;
        r[12..16].copy_from_slice(&self.seed.to_le_bytes());
        r[16..20].copy_from_slice(&self.carried_ms.to_le_bytes());
        r[20..28].copy_from_slice(&self.wait_until_ms.to_le_bytes());
        // Checksum over bytes 0..28 stored in 28..32 (FNV-1a 32-bit, zero alloc)
        let crc = fnv1a_32(&r[..28]);
        r[28..32].copy_from_slice(&crc.to_le_bytes());
        r
    }

    pub fn decode(r: Record) -> Option<Self> {
        if r[0] != RECORD_VERSION {
            return None;
        }
        let expected = u32::from_le_bytes([r[28], r[29], r[30], r[31]]);
        if fnv1a_32(&r[..28]) != expected {
            return None;
        }
        let phase = Phase::from_u8(r[1])?;
        let flags = r[2];
        let mut s = Self::new();
        s.phase = phase;
        s.clean_exit = (flags & 1) != 0;
        s.no_fast = (flags & 2) != 0;
        s.compacted = (flags & 4) != 0;
        s.need = Need::from_u8(r[3]);
        s.attempt = u16::from_le_bytes([r[4], r[5]]);
        s.cause = FailureKind::from_u8(r[6]);
        s.streak = r[7];
        s.streak_kind = FailureKind::from_u8(r[8]);
        s.backoff_step = r[9];
        s.offline_step = r[10];
        s.abrupt_deaths = r[11];
        s.seed = u32::from_le_bytes([r[12], r[13], r[14], r[15]]).max(1);
        s.carried_ms = u32::from_le_bytes([r[16], r[17], r[18], r[19]]);
        s.wait_until_ms = u64::from_le_bytes([
            r[20], r[21], r[22], r[23], r[24], r[25], r[26], r[27],
        ]);
        Some(s)
    }

    fn next_jitter_ms(&mut self, base_ms: u32) -> u64 {
        // xorshift32 deterministic PRNG seeded from now_ms
        let mut x = self.seed.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x.max(1);
        let span = 100 - JITTER_FLOOR_PERCENT;
        let pct = JITTER_FLOOR_PERCENT + (x % (span + 1));
        ((base_ms as u64) * (pct as u64)) / 100
    }
}

const fn fnv1a_32(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    let mut i = 0;
    while i < bytes.len() {
        h ^= bytes[i] as u32;
        h = h.wrapping_mul(0x0100_0193);
        i += 1;
    }
    h
}

/// Single entry point matching `recovery.zig::step`.
pub fn step(state: &mut State, event: Event, now_ms: u64) -> Plan {
    match event {
        Event::Start(start) => on_start(state, start, now_ms),
        Event::Responding => on_responding(state),
        Event::Succeeded => on_succeeded(state),
        Event::Failed(failure) => on_failed(state, failure, now_ms),
        Event::Tick => on_tick(state, now_ms),
        Event::UserContinue => on_user_continue(state, now_ms),
        Event::SignInClosed => on_sign_in_closed(state, now_ms),
        Event::UserCancel => on_user_cancel(state),
        Event::Closing => on_closing(state, now_ms),
    }
}

fn on_start(state: &mut State, start: Start, now_ms: u64) -> Plan {
    if let Some(rec) = start.resume_from
        && let Some(mut saved) = State::decode(rec)
    {
        saved.fast = start.fast;
        saved.started_ms = now_ms;
        if start.stale {
            saved.phase = Phase::NeedUser;
            saved.need = Some(Need::Stale);
            *state = saved;
            return Plan {
                save: Some(state.encode()),
                send: None,
                status: Some(Status {
                    kind: StatusKind::NeedUser,
                    need: Some(Need::Stale),
                    text: Text::from_str_trunc("Interrupted request is over a day old — press Enter to retry"),
                    short: Text::from_str_trunc("Stale request — Enter to retry"),
                }),
                finish: None,
            };
        }
        if !saved.clean_exit && saved.phase == Phase::Sending {
            saved.abrupt_deaths = saved.abrupt_deaths.saturating_add(1);
            if saved.abrupt_deaths >= CRASH_LOOP_AFTER {
                saved.phase = Phase::NeedUser;
                saved.need = Some(Need::CrashLoop);
                *state = saved;
                return Plan {
                    save: Some(state.encode()),
                    send: None,
                    status: Some(Status {
                        kind: StatusKind::NeedUser,
                        need: Some(Need::CrashLoop),
                        text: Text::from_str_trunc("Previous attempts crashed — press Enter to retry"),
                        short: Text::from_str_trunc("Crash loop — Enter to retry"),
                    }),
                    finish: None,
                };
            }
        }
        saved.clean_exit = false;
        saved.phase = Phase::Sending;
        saved.attempt = saved.attempt.saturating_add(1);
        saved.sent_ms = now_ms;
        *state = saved;
        return Plan {
            save: Some(state.encode()),
            send: Some(SendPlan {
                compact_first: false,
                no_fast: state.no_fast,
            }),
            status: Some(Status {
                kind: StatusKind::Retrying,
                need: None,
                text: Text::from_str_trunc("Resuming interrupted request"),
                short: Text::from_str_trunc("Resuming request"),
            }),
            finish: None,
        };
    }

    *state = State::new();
    state.phase = Phase::Sending;
    state.fast = start.fast;
    state.attempt = 1;
    state.seed = ((now_ms as u32) ^ ((now_ms >> 32) as u32)).max(1);
    state.started_ms = now_ms;
    state.sent_ms = now_ms;
    Plan {
        save: Some(state.encode()),
        send: Some(SendPlan::default()),
        // NoFalseAlarm: a new request shows nothing until something goes wrong.
        status: None,
        finish: None,
    }
}

fn on_responding(state: &mut State) -> Plan {
    state.flowing = true;
    state.polling = false;
    state.backoff_step = 0;
    state.offline_step = 0;
    state.streak = 0;
    state.streak_kind = None;
    Plan {
        status: Some(Status {
            kind: StatusKind::Clear,
            need: None,
            text: Text::empty(),
            short: Text::empty(),
        }),
        ..Plan::default()
    }
}

fn on_succeeded(state: &mut State) -> Plan {
    *state = State::new();
    Plan {
        status: Some(Status {
            kind: StatusKind::Clear,
            need: None,
            text: Text::empty(),
            short: Text::empty(),
        }),
        finish: Some(Finish::Succeeded),
        ..Plan::default()
    }
}

fn on_failed(state: &mut State, failure: Failure, now_ms: u64) -> Plan {
    state.flowing = false;
    state.cause = Some(failure.kind);

    if failure.kind != FailureKind::Offline {
        if state.streak_kind == Some(failure.kind) {
            state.streak = state.streak.saturating_add(1);
        } else {
            state.streak_kind = Some(failure.kind);
            state.streak = 1;
        }
    }

    // Escalate off Fast route after ESCALATE_AFTER consecutive failures (except Refused).
    if state.fast
        && !state.no_fast
        && failure.kind != FailureKind::Refused
        && state.streak >= ESCALATE_AFTER
        && matches!(
            failure.kind,
            FailureKind::Dropped
                | FailureKind::Server
                | FailureKind::RateLimited
                | FailureKind::Unreadable
        )
    {
        state.no_fast = true;
    }

    match failure.kind {
        FailureKind::TooLong => {
            if !state.compacted {
                state.compacted = true;
                state.phase = Phase::Sending;
                state.attempt = state.attempt.saturating_add(1);
                state.sent_ms = now_ms;
                return Plan {
                    save: Some(state.encode()),
                    send: Some(SendPlan {
                        compact_first: true,
                        no_fast: state.no_fast,
                    }),
                    status: Some(Status {
                        kind: StatusKind::Retrying,
                        need: None,
                        text: Text::from_str_trunc("Context too long — compacting and retrying"),
                        short: Text::from_str_trunc("Compacting & retrying"),
                    }),
                    finish: None,
                };
            }
            enter_need_user(state, Need::TooLong, "Context still too long — press Enter to retry", now_ms)
        }
        FailureKind::SignIn => {
            if state.continue_pending {
                state.continue_pending = false;
                state.polling = false;
            }
            if state.asked_ms == 0 {
                state.asked_ms = now_ms;
            }
            state.phase = Phase::NeedUser;
            state.need = Some(Need::SignIn);
            state.wait_until_ms = now_ms.saturating_add(SIGN_IN_POLL_MS);
            Plan {
                save: Some(state.encode()),
                send: None,
                status: Some(Status {
                    kind: StatusKind::NeedUser,
                    need: Some(Need::SignIn),
                    text: Text::from_str_trunc("Sign-in required — run fx login or press Enter"),
                    short: Text::from_str_trunc("Sign-in required"),
                }),
                finish: None,
            }
        }
        FailureKind::Quota => {
            enter_need_user(state, Need::Quota, "Quota exceeded — check billing then press Enter", now_ms)
        }
        FailureKind::ContentFilter => {
            enter_need_user(state, Need::ContentFilter, "Blocked by content filter — press Enter to retry", now_ms)
        }
        FailureKind::Rejected => {
            enter_need_user(state, Need::Rejected, "Request rejected — press Enter to retry", now_ms)
        }
        FailureKind::Offline => {
            let idx = (state.offline_step as usize).min(OFFLINE_MS.len() - 1);
            let delay = state.next_jitter_ms(OFFLINE_MS[idx]);
            state.offline_step = state.offline_step.saturating_add(1);
            state.phase = Phase::Waiting;
            state.wait_until_ms = now_ms.saturating_add(delay);
            Plan {
                save: Some(state.encode()),
                send: None,
                status: Some(Status {
                    kind: StatusKind::Waiting,
                    need: None,
                    text: Text::from_str_trunc(failure.kind.label()),
                    short: Text::from_str_trunc("Offline — retrying"),
                }),
                finish: None,
            }
        }
        FailureKind::Dropped
        | FailureKind::Server
        | FailureKind::RateLimited
        | FailureKind::Unreadable
        | FailureKind::Refused => {
            let delay = match failure.retry_after_ms {
                Some(ms) if ms > 0 && matches!(failure.kind, FailureKind::RateLimited | FailureKind::Server) => {
                    ms as u64
                }
                _ => {
                    let idx = (state.backoff_step as usize).min(BACKOFF_MS.len() - 1);
                    state.backoff_step = state.backoff_step.saturating_add(1);
                    state.next_jitter_ms(BACKOFF_MS[idx])
                }
            };
            state.phase = Phase::Waiting;
            state.wait_until_ms = now_ms.saturating_add(delay);
            Plan {
                save: Some(state.encode()),
                send: None,
                status: Some(Status {
                    kind: StatusKind::Waiting,
                    need: None,
                    text: Text::from_str_trunc(failure.kind.label()),
                    short: Text::from_str_trunc(failure.kind.label()),
                }),
                finish: None,
            }
        }
    }
}

fn enter_need_user(state: &mut State, need: Need, msg: &str, now_ms: u64) -> Plan {
    state.phase = Phase::NeedUser;
    state.need = Some(need);
    state.asked_ms = now_ms;
    Plan {
        save: Some(state.encode()),
        send: None,
        status: Some(Status {
            kind: StatusKind::NeedUser,
            need: Some(need),
            text: Text::from_str_trunc(msg),
            short: Text::from_str_trunc(&msg[..msg.len().min(SHORT_COLUMNS)]),
        }),
        finish: None,
    }
}

fn on_tick(state: &mut State, now_ms: u64) -> Plan {
    match state.phase {
        Phase::Waiting if now_ms >= state.wait_until_ms => {
            state.phase = Phase::Sending;
            state.attempt = state.attempt.saturating_add(1);
            state.sent_ms = now_ms;
            Plan {
                save: Some(state.encode()),
                send: Some(SendPlan {
                    compact_first: false,
                    no_fast: state.no_fast,
                }),
                status: Some(Status {
                    kind: StatusKind::Retrying,
                    need: None,
                    text: Text::from_str_trunc("Retrying request"),
                    short: Text::from_str_trunc("Retrying"),
                }),
                finish: None,
            }
        }
        Phase::NeedUser if state.need == Some(Need::SignIn) && now_ms >= state.wait_until_ms => {
            // SignInPoll: automatically resend after SIGN_IN_POLL_MS in case login succeeded externally.
            state.phase = Phase::Sending;
            state.polling = true;
            state.attempt = state.attempt.saturating_add(1);
            state.sent_ms = now_ms;
            Plan {
                save: Some(state.encode()),
                send: Some(SendPlan {
                    compact_first: false,
                    no_fast: state.no_fast,
                }),
                status: Some(Status {
                    kind: StatusKind::Retrying,
                    need: None,
                    text: Text::from_str_trunc("Checking sign-in status"),
                    short: Text::from_str_trunc("Checking sign-in"),
                }),
                finish: None,
            }
        }
        Phase::Sending if !state.flowing && state.attempt > 1 && now_ms.saturating_sub(state.sent_ms) >= HANDOFF_MS => {
            Plan {
                status: Some(Status {
                    kind: StatusKind::Clear,
                    need: None,
                    text: Text::empty(),
                    short: Text::empty(),
                }),
                ..Plan::default()
            }
        }
        _ => Plan::default(),
    }
}

fn on_user_continue(state: &mut State, now_ms: u64) -> Plan {
    if state.phase == Phase::Sending && state.polling {
        state.continue_pending = true;
        return Plan::default();
    }
    state.phase = Phase::Sending;
    state.need = None;
    state.asked_ms = 0;
    state.attempt = state.attempt.saturating_add(1);
    state.sent_ms = now_ms;
    Plan {
        save: Some(state.encode()),
        send: Some(SendPlan {
            compact_first: false,
            no_fast: state.no_fast,
        }),
        status: Some(Status {
            kind: StatusKind::Retrying,
            need: None,
            text: Text::from_str_trunc("Retrying request"),
            short: Text::from_str_trunc("Retrying"),
        }),
        finish: None,
    }
}

fn on_sign_in_closed(state: &mut State, now_ms: u64) -> Plan {
    if state.phase == Phase::NeedUser && state.need == Some(Need::SignIn) {
        return on_user_continue(state, now_ms);
    }
    Plan::default()
}

fn on_user_cancel(state: &mut State) -> Plan {
    *state = State::new();
    Plan {
        status: Some(Status {
            kind: StatusKind::Clear,
            need: None,
            text: Text::empty(),
            short: Text::empty(),
        }),
        finish: Some(Finish::Cancelled),
        ..Plan::default()
    }
}

fn on_closing(state: &mut State, now_ms: u64) -> Plan {
    state.clean_exit = true;
    let elapsed = now_ms.saturating_sub(state.started_ms).min(u32::MAX as u64) as u32;
    state.carried_ms = state.carried_ms.saturating_add(elapsed);
    Plan {
        save: Some(state.encode()),
        ..Plan::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_false_alarm_and_record_roundtrip() {
        let mut s = State::new();
        let p = step(&mut s, Event::Start(Start::default()), 1_000);
        assert!(p.status.is_none());
        assert!(p.send.is_some());
        let rec = p.save.expect("save record");
        let decoded = State::decode(rec).expect("valid record");
        assert_eq!(decoded.attempt, 1);
    }

    #[test]
    fn too_long_compacts_once_then_needs_user() {
        let mut s = State::new();
        step(&mut s, Event::Start(Start::default()), 100);
        let p1 = step(
            &mut s,
            Event::Failed(Failure {
                kind: FailureKind::TooLong,
                retry_after_ms: None,
            }),
            200,
        );
        assert!(p1.send.expect("send").compact_first);
        let p2 = step(
            &mut s,
            Event::Failed(Failure {
                kind: FailureKind::TooLong,
                retry_after_ms: None,
            }),
            300,
        );
        assert_eq!(p2.status.expect("status").need, Some(Need::TooLong));
    }

    #[test]
    fn sign_in_polls_after_180s_and_fast_drops_after_escalate() {
        let mut s = State::new();
        step(
            &mut s,
            Event::Start(Start {
                fast: true,
                ..Start::default()
            }),
            1_000,
        );
        for i in 0..ESCALATE_AFTER {
            let t = 2_000 + (i as u64) * 50_000;
            step(
                &mut s,
                Event::Failed(Failure {
                    kind: FailureKind::Server,
                    retry_after_ms: Some(1_000),
                }),
                t,
            );
            let p = step(&mut s, Event::Tick, t + 1_000);
            if i + 1 == ESCALATE_AFTER {
                assert!(p.send.expect("send").no_fast);
            }
        }
        step(
            &mut s,
            Event::Failed(Failure {
                kind: FailureKind::SignIn,
                retry_after_ms: None,
            }),
            200_000,
        );
        let poll = step(&mut s, Event::Tick, 200_000 + SIGN_IN_POLL_MS);
        assert!(poll.send.is_some());
    }
}
