use crate::intent::GlobalConfig;
use anyhow::{bail, Context, Result};
use serde_json::Value;

pub struct Api {
    base: String,
    secret: String,
}

impl Api {
    pub fn new(global: &GlobalConfig) -> Self {
        Self {
            base: format!("http://{}", global.controller),
            secret: global.secret.clone(),
        }
    }

    fn req(&self, method: &str, path: &str) -> Result<ureq::Request> {
        let url = format!("{}{}", self.base, path);
        let mut r = match method {
            "GET" => ureq::get(&url),
            "PUT" => ureq::put(&url),
            "PATCH" => ureq::request("PATCH", &url),
            "DELETE" => ureq::delete(&url),
            "POST" => ureq::post(&url),
            _ => bail!("bad method"),
        };
        if !self.secret.is_empty() {
            r = r.set("Authorization", &format!("Bearer {}", self.secret));
        }
        Ok(r)
    }

    pub fn version(&self) -> Result<String> {
        let v: Value = self
            .req("GET", "/version")?
            .call()
            .context("mihomo API unreachable; is it up?")?
            .into_json()?;
        Ok(v["version"].as_str().unwrap_or("?").to_string())
    }

    pub fn proxies(&self) -> Result<Value> {
        let v: Value = self
            .req("GET", "/proxies")?
            .call()
            .context("GET /proxies")?
            .into_json()?;
        Ok(v)
    }

    pub fn select(&self, group: &str, name: &str) -> Result<()> {
        let path = format!("/proxies/{}", urlencoding_path(group));
        let body = serde_json::json!({ "name": name });
        let status = self
            .req("PUT", &path)?
            .send_json(body)
            .context("PUT /proxies")?;
        if status.status() >= 300 {
            bail!("select failed: HTTP {}", status.status());
        }
        Ok(())
    }

    pub fn delay(&self, name: &str, timeout_ms: u64) -> Result<u64> {
        let path = format!(
            "/proxies/{}/delay?timeout={}&url={}",
            urlencoding_path(name),
            timeout_ms,
            "https%3A%2F%2Fwww.gstatic.com%2Fgenerate_204"
        );
        let v: Value = self
            .req("GET", &path)?
            .call()
            .with_context(|| format!("delay {name}"))?
            .into_json()?;
        Ok(v["delay"].as_u64().unwrap_or(0))
    }

    pub fn reload(&self, config_path: &str) -> Result<()> {
        // mihomo: PUT /configs?force=true with {"path": "..."}
        let body = serde_json::json!({ "path": config_path });
        match self.req("PUT", "/configs?force=true")?.send_json(&body) {
            Ok(resp) if resp.status() < 300 => Ok(()),
            Ok(resp) => bail!("reload HTTP {}", resp.status()),
            Err(ureq::Error::Status(code, _)) => bail!("reload HTTP {code}"),
            Err(e) => Err(e.into()),
        }
    }

}

/// Minimal path-segment encode for proxy names (utf-8 percent).
fn urlencoding_path(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
