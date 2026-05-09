use crate::error::{ResipError, ResipResult};
use crate::state::State;
use std::process::Command;

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

pub fn kill_pid(pid: u32) -> ResipResult<()> {
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

pub fn current_timestamp() -> ResipResult<String> {
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

fn expected_ssh_process(state: &State) -> bool {
    let Some(command_line) = process_command_line(state.pid) else {
        // Some platforms may hide process details. If the PID is alive but
        // unreadable, avoid a false stale result and let stop/status proceed.
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
        // Older state files only stored the port, so this is a weaker check.
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
