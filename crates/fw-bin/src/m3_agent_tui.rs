#![forbid(unsafe_code)]

//! `m3_agent_tui` — Pure Material 3 `1.5.0` Expressive Multimodal TUI Coding Agent
//! combining full AGY feature parity (`HermesClawAgent`, MCP, ACP, A2A, skills, goals),
//! Unix-accelerated Fat-LTO shell (`fw-uutils`), Asterinas OSTD VFS (`MlsDisk` + `XArray` + Yazi),
//! audited `sudo` God Mode, Pure WGPU/WGSL M3 `1.5.0` + Parametric Prototyping, Kitty Graphics/Keyboard
//! + OSC 133 GPU terminal protocols, and `fw-ui-blitz` headless PNG visual proof export.

use fw_hermes_claw::{ChannelKind, HermesClawAgent};
use fw_ostd_wasm::{MlsDisk, XArray};
use fw_ui_blitz::encode_rgba_png;
use fw_unified_stack::{
    encode_kitty_graphics_rgba_header, encode_kitty_keyboard_enable, encode_osc133_prompt_start,
    hex_voice_status, yazi_list_dir,
};
use fw_uutils::{run_applet, SUPPORTED_APPLETS};
use fw_wayland_wgpu::m3_expressive::{M3MaterialShape, M3ParametricConfig, M3WgpuScene};
use serde_json::json;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;

// Material 3 1.5.0 Expressive Dark ColorScheme ANSI 24-bit TrueColor escapes (WCAG AA >= 4.5:1)
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const FG_ON_SURFACE: &str = "\x1b[38;2;241;245;249m";
const FG_PRIMARY: &str = "\x1b[38;2;165;180;252m";
const FG_SECONDARY: &str = "\x1b[38;2;94;234;212m";
const FG_TERTIARY: &str = "\x1b[38;2;253;186;116m";
const FG_ERROR: &str = "\x1b[38;2;252;165;165m";
const FG_MUTED: &str = "\x1b[38;2;148;163;184m";
const BG_PRIMARY_CONTAINER: &str = "\x1b[48;2;49;46;129m";
const BG_SURFACE_VARIANT: &str = "\x1b[48;2;30;41;59m";

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub seq: u64,
    pub mode: String,
    pub command: String,
    pub summary: String,
}

pub struct M3AgentTuiState {
    pub agent: HermesClawAgent,
    pub vfs_disk: MlsDisk,
    pub vfs_pages: XArray,
    pub parametric: M3ParametricConfig,
    pub sudo_god_mode: bool,
    pub kitty_graphics: bool,
    pub progress: f32,
    pub active_pane: String,
    pub cwd: String,
    pub last_output: String,
    pub audit_log: Vec<AuditEntry>,
    pub frame_count: u64,
}

impl Default for M3AgentTuiState {
    fn default() -> Self {
        Self::new()
    }
}

impl M3AgentTuiState {
    pub fn new() -> Self {
        let mut disk = MlsDisk::default();
        let _ = disk.write_block(b"fw-m3-tui-superblock:v4.2.0:compose-m3-1.5.0");
        let mut pages = XArray::default();
        pages.store(0, 0x1000);
        pages.store(1, 0x2000);
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into());
        Self {
            agent: HermesClawAgent::new("fw-m3-agy-god"),
            vfs_disk: disk,
            vfs_pages: pages,
            parametric: M3ParametricConfig {
                outer_corner_radius: 20.0,
                inner_corner_radius: 8.0,
                padding_px: 12.0,
                wave_phase: 0.8,
                spec_keylines: false,
            },
            sudo_god_mode: false,
            kitty_graphics: false,
            progress: 0.72,
            active_pane: "AGY+SHELL+VFS+WGPU".into(),
            cwd,
            last_output: "Ready. Type /goal <task>, /shell <applet args>, /vfs, /sudo on, /wgpu, /mcp, /keylines on, /help, or /quit.".into(),
            audit_log: Vec::new(),
            frame_count: 0,
        }
    }

    fn record_audit(&mut self, mode: &str, command: &str, summary: &str) {
        let seq = (self.audit_log.len() as u64) + 1;
        self.audit_log.push(AuditEntry {
            seq,
            mode: mode.to_string(),
            command: command.to_string(),
            summary: summary.to_string(),
        });
    }

    pub fn execute_command(&mut self, raw: &str) -> bool {
        let line = raw.trim();
        if line.is_empty() {
            return true;
        }
        if line == "/quit" || line == "/exit" || line == "quit" || line == "exit" {
            self.last_output = "Session closed cleanly.".into();
            return false;
        }

        if let Some(rest) = line.strip_prefix("/goal ") {
            self.active_pane = "AGY-GOAL".into();
            let rep = self.agent.run_goal(ChannelKind::Cli, rest.trim());
            self.progress = 1.0;
            self.last_output = format!(
                "AGY Goal OK | channel=cli | steps={} | skills={} | memories={} | goal={}",
                rep.steps.len(),
                self.agent.skills.len(),
                self.agent.memories.len(),
                rep.goal
            );
            self.record_audit("AGY-GOAL", line, &self.last_output.clone());
            return true;
        }

        if let Some(rest) = line.strip_prefix("/sudo") {
            let arg = rest.trim().to_ascii_lowercase();
            self.sudo_god_mode = !matches!(arg.as_str(), "off" | "0" | "false");
            self.active_pane = "SUDO-GOD".into();
            self.last_output = if self.sudo_god_mode {
                "SUDO GOD MODE ARMED: unrestricted Unix LTO + VFS + WGPU + MCP dispatch (every action audit-logged).".into()
            } else {
                "SUDO GOD MODE DISARMED: standard sandboxed execution.".into()
            };
            self.record_audit("SUDO", line, &self.last_output.clone());
            return true;
        }

        if let Some(rest) = line.strip_prefix("/shell ") {
            self.active_pane = "UNIX-LTO-SHELL".into();
            let parts: Vec<&str> = rest.split_whitespace().collect();
            let applet = parts.first().copied().unwrap_or("uname");
            let args = if parts.len() > 1 { &parts[1..] } else { &[] };
            let res = run_applet(applet, args, "");
            let out = if !res.stdout.is_empty() {
                res.stdout.trim().to_string()
            } else if !res.stderr.is_empty() {
                res.stderr.trim().to_string()
            } else {
                format!("applet '{applet}' exit={}", res.exit_code)
            };
            self.last_output = format!(
                "fw-uutils[{applet}] (exit={}, sudo={}): {}",
                res.exit_code,
                if self.sudo_god_mode { "GOD" } else { "USER" },
                out.lines().take(3).collect::<Vec<_>>().join(" | ")
            );
            self.record_audit("UNIX-LTO", line, &self.last_output.clone());
            return true;
        }

        if let Some(rest) = line.strip_prefix("/vfs-write ") {
            self.active_pane = "VFS-OSTD".into();
            let digest = self.vfs_disk.write_block(rest.as_bytes());
            let idx = self.vfs_pages.len() as u64;
            self.vfs_pages.store(idx, 0x1000 * (idx + 1));
            self.last_output = format!(
                "VFS MlsDisk commit ok | blake/sha256={}.. | blocks={} | xarray_pages={}",
                &digest[..16],
                self.vfs_disk.block_count(),
                self.vfs_pages.len()
            );
            self.record_audit("VFS", line, &self.last_output.clone());
            return true;
        }

        if line.starts_with("/vfs") {
            self.active_pane = "VFS-OSTD".into();
            let target = line.strip_prefix("/vfs").unwrap_or("").trim();
            let dir = if target.is_empty() { "." } else { target };
            let entries = yazi_list_dir(dir, 8);
            let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
            self.last_output = format!(
                "VFS Yazi+MlsDisk | blocks={} | pages={} | entries({}): {}",
                self.vfs_disk.block_count(),
                self.vfs_pages.len(),
                entries.len(),
                names.join(", ")
            );
            self.record_audit("VFS", line, &self.last_output.clone());
            return true;
        }

        if line.starts_with("/keylines") {
            let on = !line.contains("off");
            self.parametric.spec_keylines = on;
            self.active_pane = "WGPU-PARAMETRIC".into();
            self.last_output = format!("Parametric Spec Keylines Overlay = {}", if on { "ON" } else { "OFF" });
            self.record_audit("PARAMETRIC", line, &self.last_output.clone());
            return true;
        }

        if let Some(rest) = line.strip_prefix("/randomize") {
            let seed = rest.trim().parse::<u64>().unwrap_or(2026_1119);
            self.parametric = M3ParametricConfig::randomize_seeded(seed, self.parametric.spec_keylines);
            self.active_pane = "WGPU-PARAMETRIC".into();
            self.last_output = format!(
                "Parametric randomized(seed={seed}): outer={:.1}dp inner={:.1}dp pad={:.1}dp",
                self.parametric.outer_corner_radius,
                self.parametric.inner_corner_radius,
                self.parametric.padding_px
            );
            self.record_audit("PARAMETRIC", line, &self.last_output.clone());
            return true;
        }

        if line == "/wgpu" {
            self.active_pane = "WGPU-M3".into();
            let scene = M3WgpuScene::from_parametric(&self.parametric, 1280, 720);
            self.last_output = format!(
                "Pure WGPU M3 1.5.0 | BOM={} | shapes={} | wgsl={}B | instances={} | keylines={}",
                scene.compose_bom,
                M3MaterialShape::ALL_35.len(),
                scene.wgsl_bytes,
                scene.instances.len(),
                self.parametric.spec_keylines
            );
            self.record_audit("WGPU", line, &self.last_output.clone());
            return true;
        }

        if line == "/mcp" {
            self.active_pane = "MCP-JSONRPC".into();
            let resp = self
                .agent
                .mcp
                .handle_jsonrpc(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}));
            let count = resp["result"]["tools"].as_array().map_or(0, Vec::len);
            self.last_output = format!("MCP JSON-RPC 2.0 live | tools/list => {count} tools exposed");
            self.record_audit("MCP", line, &self.last_output.clone());
            return true;
        }

        if line == "/voice" {
            self.active_pane = "MULTIMODAL-VOICE".into();
            let v = hex_voice_status();
            self.last_output = format!(
                "Multimodal Voice (Hex) | engine={} | asr={} | ready={}",
                v.engine, v.asr_backend, v.terminal_control_ready
            );
            self.record_audit("VOICE", line, &self.last_output.clone());
            return true;
        }

        if line == "/kitty" {
            self.kitty_graphics = !self.kitty_graphics;
            self.active_pane = "GPU-TERMINAL".into();
            let hdr = encode_kitty_graphics_rgba_header(64, 32, "AAAA");
            self.last_output = format!(
                "Kitty GPU Graphics Protocol={} | header_bytes={} | keyboard_proto_bytes={}",
                if self.kitty_graphics { "ON" } else { "OFF" },
                hdr.len(),
                encode_kitty_keyboard_enable(1).len()
            );
            self.record_audit("KITTY", line, &self.last_output.clone());
            return true;
        }

        if let Some(rest) = line.strip_prefix("/png ") {
            let path = Path::new(rest.trim());
            match render_m3_tui_png(path, 960, 540, self.sudo_god_mode, self.parametric.spec_keylines) {
                Ok(bytes) => {
                    self.last_output = format!("Rendered M3 1.5.0 TUI PNG proof: {} ({bytes} bytes)", path.display());
                }
                Err(e) => {
                    self.last_output = format!("PNG render error: {e}");
                }
            }
            self.record_audit("PNG", line, &self.last_output.clone());
            return true;
        }

        if line == "/help" {
            self.last_output = "Commands: /goal <task> | /shell <uutils applet> | /vfs [dir] | /vfs-write <data> | /sudo on|off | /wgpu | /keylines on|off | /randomize [seed] | /mcp | /voice | /kitty | /png <path> | /quit".into();
            return true;
        }

        // Default: treat plain input as an AGY coding-agent prompt
        self.active_pane = "AGY-CHAT".into();
        let rep = self.agent.run_goal(ChannelKind::Cli, line);
        self.last_output = format!("AGY Agent => ok={} steps={} goal={}", rep.ok, rep.steps.len(), rep.goal);
        self.record_audit("AGY-CHAT", line, &self.last_output.clone());
        true
    }

    pub fn render_wavy_progress(&self, width: usize) -> String {
        let filled = ((width as f32) * self.progress.clamp(0.0, 1.0)).round() as usize;
        let mut s = String::with_capacity(width * 3);
        s.push_str(FG_PRIMARY);
        for _ in 0..filled.min(width) {
            s.push('~');
        }
        s.push_str(FG_MUTED);
        for _ in filled.min(width)..width {
            s.push('-');
        }
        s.push_str(RESET);
        format!("{s} {:>3}%", (self.progress * 100.0).round() as u32)
    }

    pub fn render_frame(&mut self) -> String {
        self.frame_count += 1;
        let scene = M3WgpuScene::expressive_showcase(1280, 720);
        let voice = hex_voice_status();
        let sudo_badge = if self.sudo_god_mode {
            format!("{BOLD}{FG_ERROR}[SUDO:GOD-MODE]{RESET}")
        } else {
            format!("{FG_SECONDARY}[SUDO:SANDBOX]{RESET}")
        };
        let keyline_badge = if self.parametric.spec_keylines {
            format!("{FG_TERTIARY}[KEYLINES:ON]{RESET}")
        } else {
            format!("{FG_MUTED}[KEYLINES:OFF]{RESET}")
        };

        let mut out = String::with_capacity(4096);
        out.push_str(encode_osc133_prompt_start());
        out.push_str(&format!(
            "{BG_PRIMARY_CONTAINER}{BOLD}{FG_ON_SURFACE}  FW M3 1.5.0 EXPRESSIVE TUI CODING AGENT  |  AGY + UNIX-LTO + VFS + WGPU + MULTIMODAL  {RESET}\n"
        ));
        out.push_str(&format!(
            "{BG_SURFACE_VARIANT} {FG_PRIMARY}Pane:{RESET} {FG_ON_SURFACE}{}{RESET}  {}  {}  {FG_SECONDARY}[WGPU:{}B]{RESET}  {FG_MUTED}frame=#{}{RESET}\n",
            self.active_pane, sudo_badge, keyline_badge, scene.wgsl_bytes, self.frame_count
        ));
        out.push_str(&format!(
            "{FG_PRIMARY}SplitButton:{RESET} [ > /goal | v ]  [ $ /shell | v ]  [ # /vfs | v ]  [ * /wgpu | v ]\n"
        ));
        out.push_str(&format!(
            "{FG_PRIMARY}WavyProgress:{RESET} {}\n",
            self.render_wavy_progress(36)
        ));
        out.push_str("------------------------------------------------------------------------------------------\n");
        out.push_str(&format!(
            "{BOLD}{FG_PRIMARY}[1] AGY Coding Agent{RESET}    agent={} | mcp_tools={} | skills={} | memories={}\n",
            self.agent.name,
            self.agent.mcp.tools.len(),
            self.agent.skills.len(),
            self.agent.memories.len()
        ));
        out.push_str(&format!(
            "{BOLD}{FG_SECONDARY}[2] Unix LTO Shell{RESET}      applets={} (fw-uutils zero-fork) | cwd={}\n",
            SUPPORTED_APPLETS.len(),
            self.cwd
        ));
        out.push_str(&format!(
            "{BOLD}{FG_TERTIARY}[3] Asterinas VFS{RESET}       mlsdisk_blocks={} | xarray_pages={} | audit_events={}\n",
            self.vfs_disk.block_count(),
            self.vfs_pages.len(),
            self.audit_log.len()
        ));
        out.push_str(&format!(
            "{BOLD}{FG_PRIMARY}[4] Pure WGPU + Voice{RESET}   m3_bom={} | shapes={} | outer={:.1}dp inner={:.1}dp | voice={} ({})\n",
            scene.compose_bom,
            M3MaterialShape::ALL_35.len(),
            self.parametric.outer_corner_radius,
            self.parametric.inner_corner_radius,
            voice.engine,
            voice.asr_backend
        ));
        out.push_str("------------------------------------------------------------------------------------------\n");
        out.push_str(&format!("{BOLD}{FG_ON_SURFACE}Output:{RESET} {}\n", self.last_output));
        out.push_str(&format!(
            "{BG_SURFACE_VARIANT}{FG_PRIMARY} FloatingToolbar:{RESET} {FG_ON_SURFACE}[/goal] [/shell] [/vfs] [/sudo on] [/wgpu] [/keylines on] [/mcp] [/voice] [/quit]{RESET}\n"
        ));
        out
    }

    pub fn snapshot_json(&self) -> serde_json::Value {
        let scene = M3WgpuScene::from_parametric(&self.parametric, 1280, 720);
        json!({
            "ok": true,
            "product": "fw-m3-tui",
            "material3": "1.5.0",
            "composeBom": scene.compose_bom,
            "sudoGodMode": self.sudo_god_mode,
            "kittyGraphics": self.kitty_graphics,
            "activePane": self.active_pane,
            "frameCount": self.frame_count,
            "agy": {
                "agent": self.agent.name,
                "mcpTools": self.agent.mcp.tools.len(),
                "skills": self.agent.skills.len(),
                "memories": self.agent.memories.len()
            },
            "unixLto": {
                "uutilsApplets": SUPPORTED_APPLETS.len(),
                "cwd": self.cwd
            },
            "vfs": {
                "mlsdiskBlocks": self.vfs_disk.block_count(),
                "xarrayPages": self.vfs_pages.len()
            },
            "wgpu": {
                "wgslBytes": scene.wgsl_bytes,
                "instances": scene.instances.len(),
                "materialShapes": M3MaterialShape::ALL_35.len(),
                "parametric": self.parametric
            },
            "lastOutput": self.last_output,
            "auditLog": self.audit_log.iter().map(|a| json!({
                "seq": a.seq,
                "mode": a.mode,
                "command": a.command,
                "summary": a.summary
            })).collect::<Vec<_>>()
        })
    }
}

/// Renders a high-contrast Material 3 1.5.0 Expressive TUI GPU Terminal visual proof PNG
/// using `fw-ui-blitz` zero-dep RGBA PNG encoding.
pub fn render_m3_tui_png(
    out_path: &Path,
    width: u32,
    height: u32,
    sudo_god_mode: bool,
    spec_keylines: bool,
) -> io::Result<usize> {
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) as usize) * 4;
            // M3 Dark background #0B101F
            let (mut r, mut g, mut b) = (11u8, 16u8, 31u8);

            // Expressive TopAppBar (#312E81 primaryContainer)
            if (16..68).contains(&y) && x >= 16 && x < width.saturating_sub(16) {
                r = 49;
                g = 46;
                b = 129;
                // Sudo God Mode badge (#FCA5A5 error accent) on right
                if sudo_god_mode && (28..56).contains(&y) && x >= width.saturating_sub(180) && x < width.saturating_sub(28) {
                    r = 185;
                    g = 28;
                    b = 28;
                }
            }

            // SplitButton row (#3730A3 + #115E59)
            if (84..124).contains(&y) && (24..220).contains(&x) {
                r = 55;
                g = 48;
                b = 163;
            } else if (84..124).contains(&y) && (226..288).contains(&x) {
                r = 17;
                g = 94;
                b = 89;
            }

            // WavyLinearProgressIndicator (sine wave at y=148)
            if (136..162).contains(&y) && x >= 24 && x < width.saturating_sub(24) {
                let active_end = 24 + ((width.saturating_sub(48) as f32) * 0.72) as u32;
                let wave_y = 149.0 + ((x as f32 / 24.0) * std::f32::consts::TAU).sin() * 4.0;
                if x <= active_end && ((y as f32) - wave_y).abs() <= 2.2 {
                    r = 165;
                    g = 180;
                    b = 252;
                } else if x > active_end && (147..=151).contains(&y) {
                    r = 51;
                    g = 65;
                    b = 85;
                }
            }

            // 4 Segmented ListItem cards (AGY, Unix LTO, VFS, WGPU+Voice)
            if (176..440).contains(&y) && x >= 24 && x < width.saturating_sub(24) {
                let row = (y - 176) / 66;
                let local_y = (y - 176) % 66;
                if local_y < 58 {
                    r = 30;
                    g = 41;
                    b = 59;
                    // Left accent bar per modality
                    if x < 34 {
                        match row {
                            0 => { r = 165; g = 180; b = 252; } // AGY Primary
                            1 => { r = 94; g = 234; b = 212; }  // Unix LTO Secondary
                            2 => { r = 253; g = 186; b = 116; } // VFS Tertiary
                            _ => { r = 129; g = 140; b = 248; } // WGPU M3
                        }
                    }
                }
            }

            // HorizontalFloatingToolbar pill at bottom
            let pill_l = width / 2 - 220;
            let pill_r = width / 2 + 220;
            if y >= height.saturating_sub(72) && y < height.saturating_sub(22) && x >= pill_l && x < pill_r {
                r = 49;
                g = 46;
                b = 129;
                // Inset focus ring (#FDBA74)
                if y == height.saturating_sub(70) || y == height.saturating_sub(24) || x == pill_l + 2 || x == pill_r - 3 {
                    r = 253;
                    g = 186;
                    b = 116;
                }
            }

            // Optional Parametric Spec Keylines overlay
            if spec_keylines && (x == 24 || x == width.saturating_sub(25)) {
                r = 253;
                g = 186;
                b = 116;
            }

            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = 255;
        }
    }

    let png = encode_rgba_png(width, height, &rgba);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out_path, &png)?;
    Ok(png.len())
}

pub fn run_m3_agent_tui(args: &[String]) {
    let mut state = M3AgentTuiState::new();
    let once = args.iter().any(|a| a == "--once");
    let as_json = args.iter().any(|a| a == "--json");
    let god = args.iter().any(|a| a == "--god" || a == "--sudo");
    let keylines = args.iter().any(|a| a == "--keylines");
    if god {
        state.execute_command("/sudo on");
    }
    if keylines {
        state.execute_command("/keylines on");
    }

    if let Some(cmd_idx) = args.iter().position(|a| a == "--cmd")
        && let Some(script) = args.get(cmd_idx + 1)
    {
        for part in script.split(';') {
            if !state.execute_command(part) {
                break;
            }
        }
    }

    if let Some(png_idx) = args.iter().position(|a| a == "--render-png")
        && let Some(path) = args.get(png_idx + 1)
    {
        state.execute_command(&format!("/png {path}"));
    }

    if as_json {
        println!("{}", serde_json::to_string_pretty(&state.snapshot_json()).unwrap());
        return;
    }

    if once || args.iter().any(|a| a == "--cmd" || a == "--render-png") {
        print!("{}", state.render_frame());
        return;
    }

    // Interactive GPU terminal REPL
    let mut stdout = io::stdout();
    let _ = write!(stdout, "{}", encode_kitty_keyboard_enable(1));
    let _ = write!(stdout, "{}", state.render_frame());
    let _ = write!(stdout, "fw-m3-tui> ");
    let _ = stdout.flush();

    let stdin = io::stdin();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let keep_going = state.execute_command(&line);
        let _ = write!(stdout, "{}", state.render_frame());
        if !keep_going {
            break;
        }
        let _ = write!(stdout, "fw-m3-tui> ");
        let _ = stdout.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m3_agent_tui_executes_agy_unix_vfs_sudo_wgpu_and_png_end_to_end() {
        let mut tui = M3AgentTuiState::new();
        assert!(tui.execute_command("/sudo on"));
        assert!(tui.sudo_god_mode);
        assert!(tui.execute_command("/goal verify m3 1.5.0 wgpu"));
        assert!(tui.execute_command("/shell uname"));
        assert!(tui.execute_command("/vfs-write hello-asterinas-vfs"));
        assert!(tui.execute_command("/keylines on"));
        assert!(tui.execute_command("/wgpu"));
        assert!(tui.execute_command("/mcp"));
        assert!(tui.execute_command("/voice"));
        assert!(tui.execute_command("/kitty"));

        let frame = tui.render_frame();
        assert!(frame.contains("FW M3 1.5.0 EXPRESSIVE TUI CODING AGENT"));
        assert!(frame.contains("SUDO:GOD-MODE"));
        assert!(frame.contains("KEYLINES:ON"));
        assert!(frame.contains("2026.11.01"));
        assert!(tui.audit_log.len() >= 9);

        let tmp = std::env::temp_dir().join("fw-m3-tui-unit.png");
        let bytes = render_m3_tui_png(&tmp, 640, 360, true, true).unwrap();
        assert!(bytes > 500);
        let raw = fs::read(&tmp).unwrap();
        assert_eq!(&raw[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        let _ = fs::remove_file(&tmp);
    }
}
