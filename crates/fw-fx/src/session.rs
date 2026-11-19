//! Session store (`~/.fx/sessions/`), Agent conversation state, undo stack, and usage ledger.
//! Matches `vercel-labs/fx/src/core/session/`, `src/core/agents/`, and `src/usage/`.

use crate::compactor::{TurnRecord, estimate_tokens};
use crate::recovery::Record;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub requests: u64,
    pub estimated_cost_usd: f64,
}

impl UsageTotals {
    pub fn record(&mut self, input: u64, output: u64, cache_read: u64, cache_write: u64, cost_usd: f64) {
        self.input_tokens = self.input_tokens.saturating_add(input);
        self.output_tokens = self.output_tokens.saturating_add(output);
        self.cache_read_tokens = self.cache_read_tokens.saturating_add(cache_read);
        self.cache_write_tokens = self.cache_write_tokens.saturating_add(cache_write);
        self.requests = self.requests.saturating_add(1);
        self.estimated_cost_usd += cost_usd;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoEntry {
    pub path: String,
    pub previous_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub id: String,
    pub title: String,
    pub workspace: String,
    pub model: String,
    pub provider: String,
    pub turns: Vec<TurnRecord>,
    pub totals: UsageTotals,
    pub undo_stack: Vec<UndoEntry>,
    pub recovery_record: Option<Vec<u8>>,
}

impl SessionState {
    pub fn new(workspace: &Path, model: &str, provider: &str) -> Self {
        let mut h = Sha256::new();
        h.update(workspace.to_string_lossy().as_bytes());
        h.update(model.as_bytes());
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        h.update(nanos.to_le_bytes());
        let digest = h.finalize();
        let id = format!(
            "fx-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5]
        );
        Self {
            id,
            title: "Untitled session".to_owned(),
            workspace: workspace.display().to_string(),
            model: model.to_owned(),
            provider: provider.to_owned(),
            turns: Vec::new(),
            totals: UsageTotals::default(),
            undo_stack: Vec::new(),
            recovery_record: None,
        }
    }

    pub fn push_turn(&mut self, role: &str, content: &str) -> u64 {
        let id = (self.turns.len() as u64) + 1;
        self.turns.push(TurnRecord {
            id,
            role: role.to_owned(),
            content: content.to_owned(),
            estimated_tokens: estimate_tokens(content),
        });
        if self.title == "Untitled session" && role == "user" {
            self.title = content.chars().take(60).collect();
        }
        id
    }

    pub fn set_recovery_record(&mut self, rec: Option<Record>) {
        self.recovery_record = rec.map(|r| r.to_vec());
    }

    pub fn fork(&self, new_title: Option<&str>) -> Self {
        let mut cloned = Self::new(Path::new(&self.workspace), &self.model, &self.provider);
        cloned.title = new_title
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("{} (fork)", self.title));
        cloned.turns = self.turns.clone();
        cloned.totals = self.totals.clone();
        cloned
    }

    pub fn save_to_dir(&self, sessions_dir: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(sessions_dir).map_err(|e| e.to_string())?;
        let path = sessions_dir.join(format!("{}.json", self.id));
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(path)
    }

    pub fn load_from_dir(sessions_dir: &Path, id: &str) -> Result<Self, String> {
        let path = sessions_dir.join(format!("{id}.json"));
        let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).map_err(|e| e.to_string())
    }
}

pub fn list_sessions(sessions_dir: &Path) -> Vec<SessionSummary> {
    let Ok(rd) = std::fs::read_dir(sessions_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(&p)
            && let Ok(s) = serde_json::from_str::<SessionState>(&raw)
        {
            out.push(SessionSummary {
                id: s.id,
                title: s.title,
                workspace: s.workspace,
                turns: s.turns.len(),
                model: s.model,
            });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub workspace: String,
    pub turns: usize,
    pub model: String,
}
