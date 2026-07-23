use crate::intent::GlobalConfig;
use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use std::process::Command;

const UNIT_NAME: &str = "mihomo-cli.service";

pub fn write_unit(paths: &Paths, global: &GlobalConfig) -> Result<()> {
    let unit = paths.unit_path();
    let mihomo = resolve_bin(&global.mihomo_bin)?;
    let run_dir = paths.run_dir();
    let config = paths.current_yaml();

    // Note: AmbientCapabilities in --user units often fails (status 218) without
    // system-level setup. TUN: `sudo setcap cap_net_admin,cap_net_bind_service=+ep $(which mihomo)`
    // or run with root unit. Port-only mode needs no caps.
    let content = format!(
        r#"[Unit]
Description=mihomo (mihomo-cli)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={mihomo} -d {run_dir} -f {config}
Restart=on-failure
RestartSec=2
LimitNOFILE=1048576

[Install]
WantedBy=default.target
"#,
        mihomo = shell_escape(&mihomo),
        run_dir = shell_escape(&run_dir.to_string_lossy()),
        config = shell_escape(&config.to_string_lossy()),
    );

    if let Some(parent) = unit.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&unit, content)?;
    systemctl(&["--user", "daemon-reload"])?;
    Ok(())
}

pub fn enable_start() -> Result<()> {
    systemctl(&["--user", "enable", "--now", UNIT_NAME])?;
    Ok(())
}

pub fn stop() -> Result<()> {
    systemctl(&["--user", "stop", UNIT_NAME])?;
    Ok(())
}

pub fn is_active() -> bool {
    Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", UNIT_NAME])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn status_text() -> Result<String> {
    let out = Command::new("systemctl")
        .args(["--user", "status", UNIT_NAME, "--no-pager"])
        .output()
        .context("systemctl status")?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn restart() -> Result<()> {
    systemctl(&["--user", "restart", UNIT_NAME])?;
    Ok(())
}

fn systemctl(args: &[&str]) -> Result<()> {
    let st = Command::new("systemctl")
        .args(args)
        .status()
        .context("spawn systemctl")?;
    if !st.success() {
        bail!("systemctl {:?} failed", args);
    }
    Ok(())
}

fn resolve_bin(name: &str) -> Result<String> {
    if name.contains('/') {
        return Ok(name.to_string());
    }
    let out = Command::new("which")
        .arg(name)
        .output()
        .context("which mihomo")?;
    if !out.status.success() {
        bail!("mihomo binary not found: {name}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn shell_escape(s: &str) -> String {
    // paths only — no quotes needed if no spaces; still quote for safety
    format!("\"{}\"", s.replace('\"', "\\\""))
}

pub fn ensure_unit_exists(paths: &Paths, global: &GlobalConfig) -> Result<()> {
    if !paths.unit_path().exists() {
        write_unit(paths, global)?;
    }
    Ok(())
}
