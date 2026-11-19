//! Permission engine matching `vercel-labs/fx/src/core/permissions/permissions.zig`.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const WEB_SEARCH_PERMISSION: &str = "web_search";
pub const WEB_FETCH_PERMISSION: &str = "web_fetch";
pub const YOLO_WARNING_TEXT: &str = "Full access enabled: fx permission checks disabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Ask,
    #[default]
    Auto,
    Yolo,
}

impl PermissionMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ask" => Some(Self::Ask),
            "auto" => Some(Self::Auto),
            "yolo" | "full-access" | "full_access" | "full access" => Some(Self::Yolo),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Auto => "auto",
            Self::Yolo => "yolo",
        }
    }

    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Auto => "auto",
            Self::Yolo => "full access",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermissionDecision {
    Once,
    Always,
    Deny,
}

impl ToolPermissionDecision {
    pub const fn from_index(idx: u8) -> Self {
        match idx {
            0 => Self::Once,
            1 => Self::Always,
            _ => Self::Deny,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionTargetKind {
    None,
    PathExisting,
    PathOptionalExisting,
    PathCreateParent,
    PathExistingParent,
    CommandCwd,
    Url,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleDecision {
    Allow,
    Review,
    Ask,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionGrant {
    pub tool_name: String,
    pub target_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionEngine {
    pub mode: PermissionMode,
    pub grants: Vec<PermissionGrant>,
    pub allowlist: Vec<String>,
    pub denylist: Vec<String>,
}

impl PermissionEngine {
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            grants: Vec::new(),
            allowlist: Vec::new(),
            denylist: Vec::new(),
        }
    }

    pub fn allow(&mut self, tool_name: &str, target_path: &str) {
        if !self.is_granted(tool_name, target_path) {
            self.grants.push(PermissionGrant {
                tool_name: tool_name.to_owned(),
                target_path: target_path.to_owned(),
            });
        }
    }

    pub fn is_granted(&self, tool_name: &str, target_path: &str) -> bool {
        self.grants
            .iter()
            .any(|g| g.tool_name == tool_name && (g.target_path == "*" || g.target_path == target_path))
    }

    /// Evaluate a tool invocation against mode, rules, and workspace containment.
    pub fn evaluate(
        &self,
        workspace_root: &Path,
        tool_name: &str,
        target: &str,
        target_kind: PermissionTargetKind,
    ) -> RuleDecision {
        if self.mode == PermissionMode::Yolo {
            return RuleDecision::Allow;
        }
        if self.denylist.iter().any(|d| d == tool_name || target.starts_with(d)) {
            return RuleDecision::Deny;
        }
        if self.is_granted(tool_name, target)
            || self.allowlist.iter().any(|a| a == tool_name || a == "*")
        {
            return RuleDecision::Allow;
        }

        match self.mode {
            PermissionMode::Yolo => RuleDecision::Allow,
            PermissionMode::Ask => RuleDecision::Ask,
            PermissionMode::Auto => {
                // Read-only workspace tools are routine and allowed in auto mode.
                if matches!(tool_name, "read_file" | "glob_files" | "grep_files" | "read_tool_result") {
                    return if target_in_workspace(workspace_root, target, target_kind) {
                        RuleDecision::Allow
                    } else {
                        RuleDecision::Review
                    };
                }
                if matches!(tool_name, "edit_file" | "write_file")
                    && target_in_workspace(workspace_root, target, target_kind)
                {
                    return RuleDecision::Allow;
                }
                if tool_name == "shell" && is_routine_shell_command(target) {
                    return RuleDecision::Allow;
                }
                RuleDecision::Review
            }
        }
    }
}

fn target_in_workspace(root: &Path, target: &str, kind: PermissionTargetKind) -> bool {
    if matches!(kind, PermissionTargetKind::None | PermissionTargetKind::Url) || target.is_empty() {
        return true;
    }
    let p = Path::new(target);
    if !p.is_absolute() {
        return !target.split(['/', '\\']).any(|s| s == "..");
    }
    p.starts_with(root)
}

fn is_routine_shell_command(cmd: &str) -> bool {
    let first = cmd.split_whitespace().next().unwrap_or("");
    matches!(
        first,
        "git" | "cargo" | "bun" | "npm" | "pytest" | "zig" | "go" | "rustc"
    ) && !cmd.contains("rm -rf")
        && !cmd.contains("--force")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_modes_match_fx_contract() {
        let root = Path::new("/Users/example/project");
        let auto = PermissionEngine::new(PermissionMode::Auto);
        assert_eq!(
            auto.evaluate(root, "edit_file", "src/main.rs", PermissionTargetKind::PathExisting),
            RuleDecision::Allow
        );
        assert_eq!(
            auto.evaluate(root, "edit_file", "../outside.rs", PermissionTargetKind::PathExisting),
            RuleDecision::Review
        );
        let yolo = PermissionEngine::new(PermissionMode::Yolo);
        assert_eq!(
            yolo.evaluate(root, "shell", "rm -rf /tmp/x", PermissionTargetKind::CommandCwd),
            RuleDecision::Allow
        );
    }
}
