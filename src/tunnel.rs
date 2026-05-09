use crate::config::Config;
use crate::error::{ResipError, ResipResult};
use crate::state::State;
use crate::utils;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub fn start(config: &Config, force: bool) -> ResipResult<()> {
    if let Some(existing) = State::load_optional()? {
        match state_process_status(&existing) {
            TunnelProcessStatus::Running => {
                if !force {
                    println!("Tunnel is already running: PID {}", existing.pid);
                    print_tunnel_details(config, None);
                    let restart = utils::prompt_yes_no("Restart the existing tunnel now?", false)?;
                    if !restart {
                        println!("Kept existing SSH tunnel: PID {}", existing.pid);
                        return Ok(());
                    }
                }
                stop()?;
            }
            TunnelProcessStatus::NotRunning => {
                println!(
                    "Tunnel state was stale: PID {} is not running.",
                    existing.pid
                );
                State::remove()?;
            }
            TunnelProcessStatus::UnexpectedProcess => {
                println!(
                    "Tunnel state was stale: PID {} no longer matches the expected SSH tunnel.",
                    existing.pid
                );
                State::remove()?;
            }
        }
    }

    if !utils::command_exists("ssh") {
        return Err(ResipError::SshNotFound);
    }

    if !utils::is_port_available(&config.local_tunnel_host, config.local_tunnel_port) {
        return Err(ResipError::PortInUse {
            host: config.local_tunnel_host.clone(),
            port: config.local_tunnel_port,
        });
    }

    let identity_file = utils::expand_tilde(&config.identity_file)?;
    let forward = format!(
        "{}:{}:{}:{}",
        config.local_tunnel_host,
        config.local_tunnel_port,
        config.remote_proxy_host,
        config.remote_proxy_port
    );
    let destination = format!("{}@{}", config.ssh_user, config.ssh_host);

    let identity_arg = identity_file.to_string_lossy();
    let args = ssh_args(config, &identity_arg, &forward, &destination);

    let mut child = Command::new("ssh")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(ResipError::StartSsh)?;

    wait_for_forward(
        &mut child,
        &config.local_tunnel_host,
        config.local_tunnel_port,
    )?;

    let state = State {
        pid: child.id(),
        started_at: current_timestamp()?,
        local_tunnel_host: config.local_tunnel_host.clone(),
        local_tunnel_port: config.local_tunnel_port,
        server: format!(
            "{}@{}:{}",
            config.ssh_user, config.ssh_host, config.ssh_port
        ),
        forward: Some(forward),
        destination: Some(destination),
    };
    state.save()?;

    println!("Started SSH tunnel: PID {}", state.pid);
    print_tunnel_details(config, None);
    Ok(())
}

pub fn stop() -> ResipResult<()> {
    let Some(state) = State::load_optional()? else {
        println!("Tunnel is not running.");
        return Ok(());
    };

    match state_process_status(&state) {
        TunnelProcessStatus::Running => {
            kill_pid(state.pid)?;
            println!("Stopped SSH tunnel: PID {}", state.pid);
        }
        TunnelProcessStatus::NotRunning => {
            println!("Tunnel state was stale: PID {} is not running.", state.pid);
        }
        TunnelProcessStatus::UnexpectedProcess => {
            println!(
                "Tunnel state was stale: PID {} no longer matches the expected SSH tunnel.",
                state.pid
            );
        }
    }

    State::remove()?;
    Ok(())
}

pub fn ssh_command_string(config: &Config) -> String {
    let forward = format!(
        "{}:{}:{}:{}",
        config.local_tunnel_host,
        config.local_tunnel_port,
        config.remote_proxy_host,
        config.remote_proxy_port
    );
    format!(
        "ssh -i {} -p {} -o ExitOnForwardFailure=yes -o BatchMode=yes -o ConnectTimeout=10 -N -L {} {}@{}",
        config.identity_file, config.ssh_port, forward, config.ssh_user, config.ssh_host
    )
}

pub fn print_tunnel_details(config: &Config, pid: Option<u32>) {
    if let Some(pid) = pid {
        println!("PID: {pid}");
    }
    println!(
        "Local: {}:{} on this machine",
        config.local_tunnel_host, config.local_tunnel_port
    );
    println!(
        "SSH: {}@{}:{}",
        config.ssh_user, config.ssh_host, config.ssh_port
    );
    println!(
        "Remote target: {}:{} on remote server",
        config.remote_proxy_host, config.remote_proxy_port
    );
}

pub fn is_pid_running(pid: u32) -> bool {
    if cfg!(windows) {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .output()
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
    } else {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .is_ok_and(|status| status.success())
    }
}

pub enum TunnelProcessStatus {
    Running,
    NotRunning,
    UnexpectedProcess,
}

pub fn state_process_status(state: &State) -> TunnelProcessStatus {
    if !is_pid_running(state.pid) {
        return TunnelProcessStatus::NotRunning;
    }

    if expected_ssh_process(state) {
        TunnelProcessStatus::Running
    } else {
        TunnelProcessStatus::UnexpectedProcess
    }
}

fn ssh_args(config: &Config, identity_file: &str, forward: &str, destination: &str) -> Vec<String> {
    vec![
        "-i".to_string(),
        identity_file.to_string(),
        "-p".to_string(),
        config.ssh_port.to_string(),
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
        "-N".to_string(),
        "-L".to_string(),
        forward.to_string(),
        destination.to_string(),
    ]
}

fn wait_for_forward(
    child: &mut std::process::Child,
    local_host: &str,
    local_port: u16,
) -> ResipResult<()> {
    for _ in 0..20 {
        if let Some(status) = child.try_wait().map_err(ResipError::StartSsh)? {
            return Err(ResipError::SshExitedImmediately {
                reason: status.to_string(),
            });
        }

        if !utils::is_port_available(local_host, local_port) {
            return Ok(());
        }

        thread::sleep(Duration::from_millis(100));
    }

    Err(ResipError::SshForwardNotReady {
        host: local_host.to_string(),
        port: local_port,
    })
    .inspect_err(|_| {
        let _ = child.kill();
        let _ = child.wait();
    })
}

fn expected_ssh_process(state: &State) -> bool {
    let Some(command_line) = process_command_line(state.pid) else {
        return true;
    };

    command_line_matches_state(&command_line, state)
}

fn command_line_matches_state(command_line: &str, state: &State) -> bool {
    if !command_line.contains("ssh") {
        return false;
    }

    if let Some(forward) = &state.forward {
        if !command_line.contains(forward) {
            return false;
        }
    } else if !command_line.contains(&state.local_tunnel_port.to_string()) {
        return false;
    }

    if let Some(destination) = &state.destination {
        command_line.contains(destination)
    } else {
        let server_host = state
            .server
            .rsplit_once(':')
            .map_or(&state.server[..], |(host, _)| host);
        command_line.contains(server_host)
    }
}

fn process_command_line(pid: u32) -> Option<String> {
    let output = if cfg!(windows) {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("(Get-CimInstance Win32_Process -Filter \"ProcessId={pid}\").CommandLine"),
            ])
            .output()
            .ok()?
    } else {
        Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm=", "-o", "args="])
            .output()
            .ok()?
    };

    if !output.status.success() {
        return None;
    }

    let command_line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if command_line.is_empty() {
        None
    } else {
        Some(command_line)
    }
}

fn kill_pid(pid: u32) -> ResipResult<()> {
    let status = if cfg!(windows) {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F", "/T"])
            .status()
            .map_err(|source| ResipError::RunCommand {
                program: "taskkill",
                source,
            })?
    } else {
        Command::new("kill")
            .arg(pid.to_string())
            .status()
            .map_err(|source| ResipError::RunCommand {
                program: "kill",
                source,
            })?
    };

    if !status.success() {
        return Err(ResipError::CommandFailed {
            program: if cfg!(windows) { "taskkill" } else { "kill" },
        });
    }

    Ok(())
}

fn current_timestamp() -> ResipResult<String> {
    let output = if cfg!(windows) {
        Command::new("powershell")
            .args(["-NoProfile", "-Command", "Get-Date -Format o"])
            .output()
            .map_err(|source| ResipError::RunCommand {
                program: "powershell",
                source,
            })?
    } else {
        Command::new("date")
            .arg("-u")
            .arg("+%Y-%m-%dT%H:%M:%SZ")
            .output()
            .map_err(|source| ResipError::RunCommand {
                program: "date",
                source,
            })?
    };

    if !output.status.success() {
        return Err(ResipError::CommandFailed {
            program: if cfg!(windows) { "powershell" } else { "date" },
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::command_line_matches_state;
    use crate::state::State;

    fn state() -> State {
        State {
            pid: 42,
            started_at: "2026-05-10T00:00:00Z".to_string(),
            local_tunnel_host: "127.0.0.1".to_string(),
            local_tunnel_port: 7891,
            server: "ubuntu@example.com:22".to_string(),
            forward: Some("127.0.0.1:7891:127.0.0.1:7890".to_string()),
            destination: Some("ubuntu@example.com".to_string()),
        }
    }

    #[test]
    fn command_line_matches_expected_ssh_tunnel() {
        let command_line = concat!(
            "ssh -i /Users/me/.ssh/id_ed25519 -p 22 ",
            "-o ExitOnForwardFailure=yes -o BatchMode=yes -o ConnectTimeout=10 ",
            "-N -L 127.0.0.1:7891:127.0.0.1:7890 ubuntu@example.com"
        );

        assert!(command_line_matches_state(command_line, &state()));
    }

    #[test]
    fn command_line_rejects_reused_pid() {
        assert!(!command_line_matches_state(
            "/usr/bin/python server.py",
            &state()
        ));
    }

    #[test]
    fn command_line_rejects_different_forward() {
        let command_line = "ssh -N -L 127.0.0.1:9999:127.0.0.1:7890 ubuntu@example.com";

        assert!(!command_line_matches_state(command_line, &state()));
    }
}
