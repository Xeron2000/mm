# mm

**mihomo manager** — replace Clash Verge with a thin CLI.

```bash
mm up / down / status
mm profile list | use <name>
mm node list | use <name> | delay
mm sub add <name> <clash-url> | update
mm tun on | off
```

## Install

```bash
# requires: mihomo, systemd --user, rustc
cargo install --git https://github.com/Xeron2000/mm
# or
cargo install --path .
```

Ensure `~/.cargo/bin` (or `~/.local/bin`) is on `PATH`.

## Quick start

```bash
mm init
mm sub add home 'https://example.com/sub?t=clash'
mm up
mm node delay
mm node use '🇭🇰 HK-01'
```

## Config layout

```text
~/.config/mihomo-cli/
  config.toml              # mihomo path, controller, default mixed-port
  active                   # current profile name
  profiles/<name>.toml     # intent (subscription / nodes / tun)
  profiles/<name>.rules.yaml   # optional full rules override
  runtime/<name>.yaml      # generated (do not edit)
  subs/<name>.yaml         # subscription cache
```

### Intent example

```toml
mixed_port = 7897
tun = true
subscription = "https://example.com/sub?t=clash"

[[nodes]]
name = "home-hy2"
type = "hysteria2"
server = "1.2.3.4"
port = 443
password = "secret"
udp = true
```

### Custom rules

Put `profiles/<name>.rules.yaml` with `rules:` (and optional `rule-providers:`).  
If present, the default CN-direct template is **fully replaced**.

## Design (KISS)

| Decision | Choice |
|----------|--------|
| vs GUI | replaces Clash Verge Rev |
| profiles | multi-profile, whole switch |
| intent | short TOML → full mihomo YAML |
| process | `systemd --user` unit |
| traffic | TUN + mixed-port (no DE system-proxy glue) |
| groups | single `PROXY` select |
| subs | Clash/mihomo YAML only |

Default template: fake-ip DNS, gvisor TUN, DustinWin rulesets (CN → DIRECT, else PROXY).

## TUN privilege

User systemd units often cannot grant `CAP_NET_ADMIN`. One-time:

```bash
sudo setcap cap_net_admin,cap_net_bind_service=+ep "$(which mihomo)"
```

## Coexist with Verge

Use different ports in `config.toml` / profile:

```toml
# ~/.config/mihomo-cli/config.toml
controller = "127.0.0.1:19090"
mixed_port = 17897
```

```toml
# profile
tun = false
mixed_port = 17897
```

## License

MIT
