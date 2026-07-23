use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;

pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let root = dirs::config_dir()
            .context("no config dir")?
            .join("mihomo-cli");
        Ok(Self { root })
    }

    pub fn ensure_layout(&self) -> Result<()> {
        for d in ["profiles", "runtime", "subs", "run", "run/providers", "run/ruleset"] {
            fs::create_dir_all(self.root.join(d))?;
        }
        Ok(())
    }

    pub fn config_toml(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn active_file(&self) -> PathBuf {
        self.root.join("active")
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    pub fn profile_intent(&self, name: &str) -> PathBuf {
        self.profiles_dir().join(format!("{name}.toml"))
    }

    pub fn profile_rules(&self, name: &str) -> PathBuf {
        self.profiles_dir().join(format!("{name}.rules.yaml"))
    }

    pub fn runtime_yaml(&self, name: &str) -> PathBuf {
        self.root.join("runtime").join(format!("{name}.yaml"))
    }

    pub fn current_yaml(&self) -> PathBuf {
        self.root.join("runtime").join("current.yaml")
    }

    pub fn sub_cache(&self, name: &str) -> PathBuf {
        self.root.join("subs").join(format!("{name}.yaml"))
    }

    pub fn run_dir(&self) -> PathBuf {
        self.root.join("run")
    }

    pub fn unit_path(&self) -> PathBuf {
        dirs::config_dir()
            .expect("config dir")
            .join("systemd/user/mihomo-cli.service")
    }

    pub fn read_active(&self) -> Result<String> {
        let p = self.active_file();
        if !p.exists() {
            bail!("no active profile; run: mm sub add <name> <url>");
        }
        Ok(fs::read_to_string(p)?.trim().to_string())
    }

    pub fn set_active(&self, name: &str) -> Result<()> {
        fs::write(self.active_file(), format!("{name}\n"))?;
        // point current.yaml at the generated runtime file (copy; symlink needs care on some FS)
        let src = self.runtime_yaml(name);
        if src.exists() {
            fs::copy(&src, self.current_yaml())?;
        }
        Ok(())
    }

    pub fn list_profiles(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        let dir = self.profiles_dir();
        if !dir.exists() {
            return Ok(names);
        }
        for e in fs::read_dir(dir)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("toml") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }
}

