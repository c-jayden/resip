use crate::config::Config;
use crate::error::{ResipError, ResipResult};
use crate::state::State;
use crate::utils;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub mod process;

use process::TunnelProcessStatus;

pub fn start(config: &Config, force: bool) -> ResipResult<()> {
    if let Some(existing) = State::load_optional()? {
        match process::state_process_status(&existing) {
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

    // SSH stays in the foreground from its own point of view. We detach it
    // from this CLI by closing all standard streams and keeping only the PID.
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
        started_at: process::current_timestamp()?,
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

    match process::state_process_status(&state) {
        TunnelProcessStatus::Running => {
            process::kill_pid(state.pid)?;
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
    // A spawned SSH process can still fail a moment later. Wait until it
    // actually owns the local forwarding port before writing state.
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
