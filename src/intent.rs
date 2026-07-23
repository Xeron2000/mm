use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Global CLI settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_mihomo_bin")]
    pub mihomo_bin: String,
    #[serde(default = "default_controller")]
    pub controller: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default = "default_mixed_port")]
    pub mixed_port: u16,
}

fn default_mihomo_bin() -> String {
    "mihomo".into()
}
fn default_controller() -> String {
    "127.0.0.1:9090".into()
}
fn default_mixed_port() -> u16 {
    7897
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            mihomo_bin: default_mihomo_bin(),
            controller: default_controller(),
            secret: String::new(),
            mixed_port: default_mixed_port(),
        }
    }
}

impl GlobalConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let s = fs::read_to_string(path)?;
        Ok(toml::from_str(&s).context("parse config.toml")?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// Per-profile intent (user-facing, short).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    #[serde(default)]
    pub subscription: Option<String>,
    #[serde(default)]
    pub mixed_port: Option<u16>,
    #[serde(default = "default_true")]
    pub tun: bool,
    #[serde(default)]
    pub nodes: Vec<Node>,
}

fn default_true() -> bool {
    true
}

/// Manual node. Unknown fields pass through via `extra`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub server: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sni: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_cert_verify: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub udp: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpn: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_sha256: Option<String>,
    /// Catch-all for other mihomo fields (uuid, cipher, …).
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, toml::Value>,
}

impl Intent {
    pub fn load(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Ok(toml::from_str(&s).context("parse intent toml")?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn new_with_sub(url: &str) -> Self {
        Self {
            subscription: Some(url.to_string()),
            mixed_port: None,
            tun: true,
            nodes: vec![],
        }
    }
}

impl Node {
    /// Convert to a YAML mapping suitable for mihomo `proxies:` entry.
    pub fn to_yaml_value(&self) -> serde_yaml::Value {
        let mut map = serde_yaml::Mapping::new();
        map.insert("name".into(), self.name.clone().into());
        map.insert("type".into(), self.node_type.clone().into());
        map.insert("server".into(), self.server.clone().into());
        map.insert("port".into(), self.port.into());
        if let Some(ref p) = self.password {
            map.insert("password".into(), p.clone().into());
        }
        if let Some(ref s) = self.sni {
            map.insert("sni".into(), s.clone().into());
        }
        if let Some(v) = self.skip_cert_verify {
            map.insert("skip-cert-verify".into(), v.into());
        }
        if let Some(v) = self.udp {
            map.insert("udp".into(), v.into());
        }
        if let Some(ref u) = self.up {
            map.insert("up".into(), u.clone().into());
        }
        if let Some(ref d) = self.down {
            map.insert("down".into(), d.clone().into());
        }
        if let Some(ref a) = self.alpn {
            let seq: Vec<serde_yaml::Value> = a.iter().map(|s| s.clone().into()).collect();
            map.insert("alpn".into(), seq.into());
        }
        if let Some(ref p) = self.pin_sha256 {
            map.insert("pinSHA256".into(), p.clone().into());
        }
        for (k, v) in &self.extra {
            // skip keys we already set
            let key = k.replace('_', "-");
            if matches!(
                key.as_str(),
                "name" | "type" | "server" | "port" | "password" | "sni"
                    | "skip-cert-verify" | "udp" | "up" | "down" | "alpn" | "pinSHA256"
            ) {
                continue;
            }
            map.insert(key.into(), toml_to_yaml(v));
        }
        serde_yaml::Value::Mapping(map)
    }
}

fn toml_to_yaml(v: &toml::Value) -> serde_yaml::Value {
    match v {
        toml::Value::String(s) => s.clone().into(),
        toml::Value::Integer(i) => (*i).into(),
        toml::Value::Float(f) => (*f).into(),
        toml::Value::Boolean(b) => (*b).into(),
        toml::Value::Array(a) => {
            let seq: Vec<_> = a.iter().map(toml_to_yaml).collect();
            seq.into()
        }
        toml::Value::Table(t) => {
            let mut m = serde_yaml::Mapping::new();
            for (k, v) in t {
                m.insert(k.clone().into(), toml_to_yaml(v));
            }
            serde_yaml::Value::Mapping(m)
        }
        toml::Value::Datetime(d) => d.to_string().into(),
    }
}
