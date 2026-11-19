//! Context compaction engine matching `vercel-labs/fx/src/core/compactor/`.
//! Keeps full turn text on disk in the session store and projects a compact
//! summary + recent tail turns for the model's context window.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRecord {
    pub id: u64,
    pub role: String,
    pub content: String,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub compacted_turn_count: usize,
    pub kept_tail_count: usize,
    pub summary_message: String,
    pub saved_tokens: usize,
}

pub fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}

pub fn should_compact(used_tokens: usize, context_window: usize, auto_compact_percent: u8) -> bool {
    if context_window == 0 {
        return false;
    }
    let threshold = (context_window as u128 * auto_compact_percent as u128) / 100;
    (used_tokens as u128) >= threshold
}

/// Compact older turns into a structured summary while preserving the newest `keep_tail` turns.
pub fn compact_turns(turns: &[TurnRecord], keep_tail: usize) -> (CompactionResult, Vec<TurnRecord>) {
    if turns.len() <= keep_tail {
        return (
            CompactionResult {
                compacted_turn_count: 0,
                kept_tail_count: turns.len(),
                summary_message: String::new(),
                saved_tokens: 0,
            },
            turns.to_vec(),
        );
    }

    let split_at = turns.len() - keep_tail;
    let older = &turns[..split_at];
    let tail = &turns[split_at..];

    let older_tokens: usize = older.iter().map(|t| t.estimated_tokens).sum();
    let mut summary = String::from("[Compacted earlier conversation turns — full text retained on disk]\n");
    for t in older {
        let preview: String = t.content.chars().take(120).collect();
        summary.push_str(&format!("- turn#{} ({}): {}\n", t.id, t.role, preview.replace('\n', " ")));
    }
    let summary_tokens = estimate_tokens(&summary);
    let saved_tokens = older_tokens.saturating_sub(summary_tokens);

    let mut projected = Vec::with_capacity(tail.len() + 1);
    projected.push(TurnRecord {
        id: 0,
        role: "system".to_owned(),
        content: summary.clone(),
        estimated_tokens: summary_tokens,
    });
    projected.extend_from_slice(tail);

    (
        CompactionResult {
            compacted_turn_count: older.len(),
            kept_tail_count: tail.len(),
            summary_message: summary,
            saved_tokens,
        },
        projected,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compacts_older_turns_and_preserves_tail() {
        let turns: Vec<TurnRecord> = (1..=8)
            .map(|id| TurnRecord {
                id,
                role: if id % 2 == 1 { "user" } else { "assistant" }.to_owned(),
                content: "x".repeat(400),
                estimated_tokens: 100,
            })
            .collect();
        let (res, projected) = compact_turns(&turns, 3);
        assert_eq!(res.compacted_turn_count, 5);
        assert_eq!(res.kept_tail_count, 3);
        assert_eq!(projected.len(), 4);
        assert!(res.saved_tokens > 0);
    }
}
