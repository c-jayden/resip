use crate::config::Config;
use crate::error::{ResipError, ResipResult};
use crate::state::State;
use crate::utils;
use std::process::{Command, Stdio};

pub fn start(config: &Config, force: bool) -> ResipResult<()> {
    if let Some(existing) = State::load_optional()? {
        if is_pid_running(existing.pid) {
            if !force {
                println!("Tunnel is already running: PID {}", existing.pid);
                print_tunnel_details(config, None);
                return Ok(());
            }
            stop()?;
        } else {
            State::remove()?;
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

    let child = Command::new("ssh")
        .arg("-i")
        .arg(identity_file)
        .arg("-p")
        .arg(config.ssh_port.to_string())
        .arg("-o")
        .arg("ExitOnForwardFailure=yes")
        .arg("-N")
        .arg("-L")
        .arg(forward)
        .arg(destination)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(ResipError::StartSsh)?;

    let state = State {
        pid: child.id(),
        started_at: current_timestamp()?,
        local_tunnel_host: config.local_tunnel_host.clone(),
        local_tunnel_port: config.local_tunnel_port,
        server: format!(
            "{}@{}:{}",
            config.ssh_user, config.ssh_host, config.ssh_port
        ),
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

    if is_pid_running(state.pid) {
        kill_pid(state.pid)?;
        println!("Stopped SSH tunnel: PID {}", state.pid);
    } else {
        println!("Tunnel state was stale: PID {} is not running.", state.pid);
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
        "ssh -i {} -p {} -o ExitOnForwardFailure=yes -N -L {} {}@{}",
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
            .map_or(false, |output| {
                String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
            })
    } else {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map_or(false, |status| status.success())
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
