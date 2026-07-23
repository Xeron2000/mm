mod api;
mod generate;
mod intent;
mod paths;
mod sub;
mod systemd;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use intent::{GlobalConfig, Intent};
use paths::Paths;
use std::thread;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "mm", about = "mihomo manager — profiles / sub / tun / nodes")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create dirs, default config, systemd user unit
    Init,
    /// Start mihomo (systemd --user)
    Up,
    /// Stop mihomo
    Down,
    /// Show service + API status
    Status,
    /// Regenerate active profile and reload
    Reload,
    /// TUN on/off for active profile
    Tun {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
    /// Subscription helpers
    Sub {
        #[command(subcommand)]
        action: SubCmd,
    },
    /// Profile management
    Profile {
        #[command(subcommand)]
        action: ProfileCmd,
    },
    /// Nodes in PROXY group
    Node {
        #[command(subcommand)]
        action: NodeCmd,
    },
}

#[derive(Subcommand)]
enum SubCmd {
    /// Create profile from subscription URL, fetch, generate, activate
    Add {
        name: String,
        url: String,
    },
    /// List profiles that have a subscription
    List,
    /// Activate profile (same as profile use)
    Use {
        name: String,
    },
    /// Re-download subscription cache
    Update {
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum ProfileCmd {
    List,
    Use {
        name: String,
    },
    /// Generate runtime yaml (no reload)
    Gen {
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum NodeCmd {
    List,
    Use {
        name: String,
    },
    /// Delay test PROXY group members
    Delay {
        #[arg(long, default_value_t = 3000)]
        timeout: u64,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let paths = Paths::discover()?;

    match cli.cmd {
        Cmd::Init => cmd_init(&paths),
        Cmd::Up => cmd_up(&paths),
        Cmd::Down => {
            systemd::stop()?;
            println!("stopped");
            Ok(())
        }
        Cmd::Status => cmd_status(&paths),
        Cmd::Reload => cmd_reload(&paths),
        Cmd::Tun { state } => cmd_tun(&paths, &state),
        Cmd::Sub { action } => match action {
            SubCmd::Add { name, url } => cmd_sub_add(&paths, &name, &url),
            SubCmd::List => cmd_sub_list(&paths),
            SubCmd::Use { name } => cmd_profile_use(&paths, &name),
            SubCmd::Update { name } => cmd_sub_update(&paths, name.as_deref()),
        },
        Cmd::Profile { action } => match action {
            ProfileCmd::List => cmd_profile_list(&paths),
            ProfileCmd::Use { name } => cmd_profile_use(&paths, &name),
            ProfileCmd::Gen { name } => cmd_profile_gen(&paths, name.as_deref()),
        },
        Cmd::Node { action } => match action {
            NodeCmd::List => cmd_node_list(&paths),
            NodeCmd::Use { name } => cmd_node_use(&paths, &name),
            NodeCmd::Delay { timeout } => cmd_node_delay(&paths, timeout),
        },
    }
}

fn load_global(paths: &Paths) -> Result<GlobalConfig> {
    GlobalConfig::load(&paths.config_toml())
}

fn cmd_init(paths: &Paths) -> Result<()> {
    paths.ensure_layout()?;
    let cfg_path = paths.config_toml();
    if !cfg_path.exists() {
        GlobalConfig::default().save(&cfg_path)?;
        println!("wrote {}", cfg_path.display());
    } else {
        println!("exists {}", cfg_path.display());
    }
    let global = load_global(paths)?;
    systemd::write_unit(paths, &global)?;
    println!("wrote {}", paths.unit_path().display());
    println!("ok — next: mm sub add <name> <clash-url>");
    Ok(())
}

fn cmd_up(paths: &Paths) -> Result<()> {
    paths.ensure_layout()?;
    let global = load_global(paths)?;
    let active = paths.read_active().context("no active profile")?;
    generate::write_runtime(paths, &global, &active)?;
    paths.set_active(&active)?; // refresh current.yaml
    systemd::ensure_unit_exists(paths, &global)?;
    // rewrite unit in case paths/bin changed
    systemd::write_unit(paths, &global)?;
    if systemd::is_active() {
        systemd::restart()?;
        println!("restarted ({active})");
    } else {
        systemd::enable_start()?;
        println!("started ({active})");
    }
    // brief wait for API
    for _ in 0..20 {
        if api::Api::new(&global).version().is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn cmd_status(paths: &Paths) -> Result<()> {
    let global = load_global(paths)?;
    let active = paths.read_active().unwrap_or_else(|_| "(none)".into());
    println!("active:  {active}");
    println!(
        "service: {}",
        if systemd::is_active() {
            "active"
        } else {
            "inactive"
        }
    );
    let api = api::Api::new(&global);
    match api.version() {
        Ok(v) => println!("api:     mihomo {v} @ {}", global.controller),
        Err(e) => println!("api:     down ({e})"),
    }
    if let Ok(proxies) = api.proxies() {
        if let Some(now) = proxies["proxies"]["PROXY"]["now"].as_str() {
            println!("node:    {now}");
        }
    }
    if std::env::var_os("MIHOMO_CLI_VERBOSE").is_some() {
        print!("{}", systemd::status_text().unwrap_or_default());
    }
    Ok(())
}

fn cmd_reload(paths: &Paths) -> Result<()> {
    let global = load_global(paths)?;
    let active = paths.read_active()?;
    let out = generate::write_runtime(paths, &global, &active)?;
    paths.set_active(&active)?;
    let api = api::Api::new(&global);
    if api.version().is_ok() {
        api.reload(&out.to_string_lossy())?;
        println!("reloaded {active}");
    } else if systemd::is_active() {
        systemd::restart()?;
        println!("restarted {active}");
    } else {
        bail!("mihomo not running; mm up");
    }
    Ok(())
}

fn cmd_tun(paths: &Paths, state: &str) -> Result<()> {
    let name = paths.read_active()?;
    let path = paths.profile_intent(&name);
    let mut intent = Intent::load(&path)?;
    intent.tun = state == "on";
    intent.save(&path)?;
    println!("tun {state} → profile {name}");
    cmd_reload(paths)
}

fn cmd_sub_add(paths: &Paths, name: &str, url: &str) -> Result<()> {
    validate_name(name)?;
    paths.ensure_layout()?;
    let intent_path = paths.profile_intent(name);
    let intent = if intent_path.exists() {
        let mut i = Intent::load(&intent_path)?;
        i.subscription = Some(url.to_string());
        i
    } else {
        Intent::new_with_sub(url)
    };
    intent.save(&intent_path)?;
    sub::fetch(paths, name, url)?;
    let global = load_global(paths)?;
    generate::write_runtime(paths, &global, name)?;
    paths.set_active(name)?;
    println!("profile '{name}' ready (active)");
    println!("start with: mm up");
    Ok(())
}

fn cmd_sub_list(paths: &Paths) -> Result<()> {
    let active = paths.read_active().ok();
    for name in paths.list_profiles()? {
        let intent = Intent::load(&paths.profile_intent(&name))?;
        if let Some(url) = intent.subscription {
            let mark = if active.as_deref() == Some(name.as_str()) {
                "*"
            } else {
                " "
            };
            println!("{mark} {name}\t{url}");
        }
    }
    Ok(())
}

fn cmd_sub_update(paths: &Paths, name: Option<&str>) -> Result<()> {
    let names: Vec<String> = match name {
        Some(n) => vec![n.to_string()],
        None => paths.list_profiles()?,
    };
    let global = load_global(paths)?;
    for n in names {
        let intent = Intent::load(&paths.profile_intent(&n))?;
        let Some(url) = intent.subscription else {
            continue;
        };
        print!("update {n} ... ");
        sub::fetch(paths, &n, &url)?;
        generate::write_runtime(paths, &global, &n)?;
        println!("ok");
    }
    if let Ok(active) = paths.read_active() {
        if name.is_none() || name == Some(active.as_str()) {
            let _ = cmd_reload(paths);
        }
    }
    Ok(())
}

fn cmd_profile_list(paths: &Paths) -> Result<()> {
    let active = paths.read_active().ok();
    for name in paths.list_profiles()? {
        let mark = if active.as_deref() == Some(name.as_str()) {
            "*"
        } else {
            " "
        };
        let intent = Intent::load(&paths.profile_intent(&name))?;
        let sub = if intent.subscription.is_some() {
            "sub"
        } else {
            "local"
        };
        let tun = if intent.tun { "tun" } else { "notun" };
        println!("{mark} {name}\t{sub}\t{tun}");
    }
    Ok(())
}

fn cmd_profile_use(paths: &Paths, name: &str) -> Result<()> {
    if !paths.profile_intent(name).exists() {
        bail!("profile '{name}' not found");
    }
    let global = load_global(paths)?;
    generate::write_runtime(paths, &global, name)?;
    paths.set_active(name)?;
    println!("active → {name}");
    if systemd::is_active() {
        let api = api::Api::new(&global);
        let cfg = paths.current_yaml();
        if api.version().is_ok() {
            api.reload(&cfg.to_string_lossy())?;
            println!("reloaded");
        } else {
            systemd::restart()?;
            println!("restarted");
        }
    } else {
        println!("(service not running; mm up)");
    }
    Ok(())
}

fn cmd_profile_gen(paths: &Paths, name: Option<&str>) -> Result<()> {
    let global = load_global(paths)?;
    let name = match name {
        Some(n) => n.to_string(),
        None => paths.read_active()?,
    };
    let out = generate::write_runtime(paths, &global, &name)?;
    println!("{}", out.display());
    Ok(())
}

fn cmd_node_list(paths: &Paths) -> Result<()> {
    let global = load_global(paths)?;
    let api = api::Api::new(&global);
    let proxies = api.proxies()?;
    let group = &proxies["proxies"]["PROXY"];
    let now = group["now"].as_str().unwrap_or("?");
    let all = group["all"].as_array().cloned().unwrap_or_default();
    for n in all {
        let name = n.as_str().unwrap_or("?");
        let mark = if name == now { "*" } else { " " };
        println!("{mark} {name}");
    }
    Ok(())
}

fn cmd_node_use(paths: &Paths, name: &str) -> Result<()> {
    let global = load_global(paths)?;
    api::Api::new(&global).select("PROXY", name)?;
    println!("PROXY → {name}");
    Ok(())
}

fn cmd_node_delay(paths: &Paths, timeout: u64) -> Result<()> {
    let global = load_global(paths)?;
    let api = api::Api::new(&global);
    let proxies = api.proxies()?;
    let group = &proxies["proxies"]["PROXY"];
    let all = group["all"].as_array().cloned().unwrap_or_default();
    for n in all {
        let name = n.as_str().unwrap_or("?");
        if name == "DIRECT" || name == "REJECT" {
            continue;
        }
        match api.delay(name, timeout) {
            Ok(ms) => println!("{ms:>5} ms  {name}"),
            Err(_) => println!("  err     {name}"),
        }
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("name must be [A-Za-z0-9_-]+");
    }
    Ok(())
}
