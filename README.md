# resip

`resip` is a small Rust 2024 CLI for managing this proxy chain:

```text
Local Clash -> 127.0.0.1:7891 -> SSH tunnel -> remote 127.0.0.1:7890 -> internet
```

The remote Ubuntu server is expected to already run Mihomo/Clash on `127.0.0.1:7890`.
That port should not be exposed publicly.

## Install

```bash
cargo build --release
```

The binary is written to `target/release/resip`.

## Quick Start

```bash
resip setup
resip on
resip test
```

`resip setup` runs `init` and `gen`.

During `init`, only the required SSH fields are asked:

- Server IP
- SSH user, default `ubuntu`
- SSH port, default `22`
- Identity file, automatically detected from `~/.ssh`

Private key detection uses the first existing file in this priority order:

1. `~/.ssh/id_ed25519`
2. `~/.ssh/id_rsa`
3. `~/.ssh/id_ecdsa`
4. `~/.ssh/id_dsa`

If none of these files exist, `init` still defaults to `~/.ssh/id_ed25519` and tells you the file does not currently exist. The saved config keeps the `~` form where possible; `resip on` expands it only when running `ssh`.

By default, the generated Clash YAML is written to your Downloads directory:

```text
~/Downloads/resip-resip-server.yaml
```

The file name is `resip-{name}.yaml`. If the Downloads directory cannot be detected, `resip` falls back to your home directory.

You can set a custom output path during init:

```bash
resip init 1.2.3.4 --output ~/Desktop/resip.yaml
```

Everything else uses defaults:

```json
{
  "name": "resip-server",
  "local_tunnel_host": "127.0.0.1",
  "local_tunnel_port": 7891,
  "remote_proxy_host": "127.0.0.1",
  "remote_proxy_port": 7890,
  "local_clash_port": 7890,
  "clash_output_path": "~/Downloads/resip-resip-server.yaml"
}
```

After `resip gen`, import the printed YAML path in your local Clash client, then enable Clash system proxy.
Use `resip open` to open the directory containing the generated YAML.
Use `resip gen --open` to generate the YAML and automatically open its directory, which is convenient when dragging the file into a Clash client for import.

## Commands

```bash
resip init          # interactive config
resip init 1.2.3.4 --output ~/Desktop/resip.yaml
resip init --force  # overwrite existing config
resip gen           # generate local Clash YAML
resip gen --open    # generate YAML and open its directory
resip open          # open the generated config directory
resip on            # start SSH tunnel in background
resip on --force    # stop existing tunnel first, then start
resip off           # stop SSH tunnel
resip status        # show config path, state, PID, ports, server
resip test          # request https://ipinfo.io/json through local Clash
resip ssh           # print equivalent ssh command
resip autostart enable   # start automatically when the current user logs in
resip autostart disable  # remove current-user autostart
resip autostart status   # show current-user autostart status
resip setup         # init + gen
```

## Long-running tunnel

`resip on` starts a small background supervisor. The supervisor starts the SSH
tunnel and restarts it if SSH exits because of an idle timeout, network switch,
or temporary server-side disconnect. `resip off` stops both the supervisor and
the current SSH process.

The SSH command also enables client-side keepalive:

```text
ServerAliveInterval=30
ServerAliveCountMax=3
```

This helps SSH detect broken idle connections quickly. The supervisor handles
the next step: reconnecting until you run `resip off`.

## Autostart

`resip autostart enable` registers `resip on` for the current user only:

- macOS: writes `~/Library/LaunchAgents/com.resip.tunnel.plist`
- Windows: writes the `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
  value `resip`
- Linux: writes `~/.config/systemd/user/resip.service` and enables it with
  `systemctl --user`

No administrator/root permission is required by design.

## Security Notes

`resip` does not store server passwords and only uses SSH key login.

Keep the remote Mihomo/Clash private. Recommended remote settings:

```yaml
allow-lan: false
bind-address: 127.0.0.1
```

Do not expose remote port `7890` to the public internet. The intended access path is SSH local forwarding only.

This first version is CLI-only. It does not provide a GUI, tray app, Clash auto-import, subscription service, or web panel.
