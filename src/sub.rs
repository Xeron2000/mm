use crate::paths::Paths;
use anyhow::{bail, Context, Result};
use std::fs;

/// Download a clash/mihomo subscription YAML and cache it.
pub fn fetch(paths: &Paths, name: &str, url: &str) -> Result<()> {
    let body = ureq::get(url)
        .set("User-Agent", "mm/0.1")
        .call()
        .with_context(|| format!("download sub {url}"))?
        .into_string()
        .context("read sub body")?;

    // cheap validate: must look like yaml with proxies or proxy-providers
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&body).context("subscription is not valid YAML (need clash format)")?;

    let ok = match &parsed {
        serde_yaml::Value::Mapping(m) => {
            m.contains_key(&serde_yaml::Value::String("proxies".into()))
                || m.contains_key(&serde_yaml::Value::String("proxy-providers".into()))
                || m.contains_key(&serde_yaml::Value::String("Proxy".into())) // some old style
        }
        serde_yaml::Value::Sequence(s) => !s.is_empty(), // bare proxy list
        _ => false,
    };
    if !ok {
        bail!("subscription YAML has no proxies / proxy-providers");
    }

    // normalize bare list into {proxies: [...]} for file provider
    let out = if matches!(parsed, serde_yaml::Value::Sequence(_)) {
        let mut m = serde_yaml::Mapping::new();
        m.insert("proxies".into(), parsed);
        serde_yaml::to_string(&serde_yaml::Value::Mapping(m))?
    } else {
        body
    };

    let path = paths.sub_cache(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, out)?;
    Ok(())
}
