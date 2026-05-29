use crate::config::Config;
use crate::error::{ResipError, ResipResult};
use crate::state::State;
use crate::utils;
use std::env;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub mod process;

use process::TunnelProcessStatus;

const FORWARD_READY_TIMEOUT: Duration = Duration::from_secs(15);
const FORWARD_READY_POLL_INTERVAL: Duration = Duration::from_millis(100);
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

pub fn start(config: &Config, force: bool) -> ResipResult<()> {
    if let Some(existing) = State::load_optional()? {
        if is_supervisor_running(&existing)
            || matches!(
                process::state_process_status(&existing),
                TunnelProcessStatus::Running
            )
        {
            if !force {
                println!("Tunnel is already managed.");
                print_state_details(&existing);
                print_tunnel_details(config, None);
                let restart = utils::prompt_yes_no("Restart the existing tunnel now?", false)?;
                if !restart {
                    println!("Kept existing tunnel.");
                    return Ok(());
                }
            }
            stop()?;
        } else {
            println!("Tunnel state was stale.");
            print_state_details(&existing);
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

    let supervisor_pid = spawn_supervisor()?;
    wait_for_supervised_forward(config, supervisor_pid)?;

    let state = State::load_optional()?.ok_or(ResipError::SshForwardNotReady {
        host: config.local_tunnel_host.clone(),
        port: config.local_tunnel_port,
    })?;
    println!("Started SSH tunnel supervisor: PID {supervisor_pid}");
    println!("SSH tunnel: PID {}", state.pid);
    print_tunnel_details(config, None);
    Ok(())
}

pub fn stop() -> ResipResult<()> {
    let Some(state) = State::load_optional()? else {
        println!("Tunnel is not running.");
        return Ok(());
    };

    if let Some(supervisor_pid) = state.supervisor_pid
        && process::is_pid_running(supervisor_pid)
    {
        process::kill_pid(supervisor_pid)?;
        println!("Stopped tunnel supervisor: PID {supervisor_pid}");
    }

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

pub fn run_supervisor() -> ResipResult<()> {
    let config = Config::load()?;
    let supervisor_pid = std::process::id();

    loop {
        match start_ssh_once(&config, Some(supervisor_pid)) {
            Ok(mut child) => {
                let ssh_pid = child.id();
                let _ = child.wait();

                let Some(state) = State::load_optional()? else {
                    return Ok(());
                };
                if state.supervisor_pid != Some(supervisor_pid) || state.pid != ssh_pid {
                    return Ok(());
                }
            }
            Err(error) => {
                eprintln!("resip supervisor: {error}");
            }
        }

        thread::sleep(RECONNECT_DELAY);
    }
}

pub fn ssh_command_string(config: &Config) -> String {
    let forward = format!(
        "{}:{}:{}:{}",
        config.local_tunnel_host,
        config.local_tunnel_port,
        config.remote_proxy_host,
        config.remote_proxy_port
    );
    let destination = format!("{}@{}", config.ssh_user, config.ssh_host);
    let args = ssh_args(config, &config.identity_file, &forward, &destination);
    format!("ssh {}", args.join(" "))
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
        "-o".to_string(),
        "ServerAliveInterval=30".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=3".to_string(),
        "-N".to_string(),
        "-L".to_string(),
        forward.to_string(),
        destination.to_string(),
    ]
}

fn start_ssh_once(
    config: &Config,
    supervisor_pid: Option<u32>,
) -> ResipResult<std::process::Child> {
    if !utils::is_port_available(&config.local_tunnel_host, config.local_tunnel_port) {
        return Err(ResipError::PortInUse {
            host: config.local_tunnel_host.clone(),
            port: config.local_tunnel_port,
        });
    }

    let identity_file = utils::expand_tilde(&config.identity_file)?;
    let forward = forward_spec(config);
    let destination = destination(config);
    let identity_arg = identity_file.to_string_lossy();
    let args = ssh_args(config, &identity_arg, &forward, &destination);

    let mut child = spawn_ssh(&args)?;
    wait_for_forward(
        &mut child,
        &config.local_tunnel_host,
        config.local_tunnel_port,
    )?;

    save_running_state(config, child.id(), forward, destination, supervisor_pid)?;
    Ok(child)
}

fn spawn_ssh(args: &[String]) -> ResipResult<std::process::Child> {
    // SSH stays in the foreground from its own point of view. We detach it
    // from this CLI by closing all standard streams and keeping only the PID.
    Command::new("ssh")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(ResipError::StartSsh)
}

fn save_running_state(
    config: &Config,
    ssh_pid: u32,
    forward: String,
    destination: String,
    supervisor_pid: Option<u32>,
) -> ResipResult<()> {
    let state = State {
        pid: ssh_pid,
        started_at: process::current_timestamp()?,
        local_tunnel_host: config.local_tunnel_host.clone(),
        local_tunnel_port: config.local_tunnel_port,
        server: format!(
            "{}@{}:{}",
            config.ssh_user, config.ssh_host, config.ssh_port
        ),
        forward: Some(forward),
        destination: Some(destination),
        supervisor_pid,
    };
    state.save()
}

fn spawn_supervisor() -> ResipResult<u32> {
    let executable = env::current_exe().map_err(ResipError::CurrentExe)?;
    let mut command = Command::new(executable);
    command
        .arg("supervisor")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach_command(&mut command);
    let child = command.spawn().map_err(ResipError::StartSupervisor)?;
    Ok(child.id())
}

fn wait_for_supervised_forward(config: &Config, supervisor_pid: u32) -> ResipResult<()> {
    let deadline = Instant::now() + FORWARD_READY_TIMEOUT;
    while Instant::now() < deadline {
        if !process::is_pid_running(supervisor_pid) {
            return Err(ResipError::SupervisorExitedImmediately);
        }

        if let Some(state) = State::load_optional()?
            && state.supervisor_pid == Some(supervisor_pid)
            && matches!(
                process::state_process_status(&state),
                TunnelProcessStatus::Running
            )
        {
            return Ok(());
        }

        thread::sleep(FORWARD_READY_POLL_INTERVAL);
    }

    let _ = process::kill_pid(supervisor_pid);
    Err(ResipError::SshForwardNotReady {
        host: config.local_tunnel_host.clone(),
        port: config.local_tunnel_port,
    })
}

fn is_supervisor_running(state: &State) -> bool {
    state.supervisor_pid.is_some_and(process::is_pid_running)
}

fn print_state_details(state: &State) {
    if let Some(supervisor_pid) = state.supervisor_pid {
        println!("Supervisor PID: {supervisor_pid}");
    }
    println!("SSH PID: {}", state.pid);
}

fn forward_spec(config: &Config) -> String {
    format!(
        "{}:{}:{}:{}",
        config.local_tunnel_host,
        config.local_tunnel_port,
        config.remote_proxy_host,
        config.remote_proxy_port
    )
}

fn destination(config: &Config) -> String {
    format!("{}@{}", config.ssh_user, config.ssh_host)
}

#[cfg(windows)]
fn detach_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    command.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS | CREATE_NO_WINDOW);
}

#[cfg(unix)]
fn detach_command(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(not(any(unix, windows)))]
fn detach_command(_command: &mut Command) {}

fn wait_for_forward(
    child: &mut std::process::Child,
    local_host: &str,
    local_port: u16,
) -> ResipResult<()> {
    // A spawned SSH process can still fail a moment later. Wait until it
    // actually owns the local forwarding port before writing state.
    let deadline = Instant::now() + FORWARD_READY_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().map_err(ResipError::StartSsh)? {
            return Err(ResipError::SshExitedImmediately {
                reason: status.to_string(),
            });
        }

        if !utils::is_port_available(local_host, local_port) {
            return Ok(());
        }

        thread::sleep(FORWARD_READY_POLL_INTERVAL);
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

#[cfg(test)]
mod tests {
    use super::{ssh_args, wait_for_forward};
    use crate::config::Config;
    use std::net::TcpListener;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn ssh_args_enable_client_keepalive() {
        let args = ssh_args(
            &Config::default(),
            "/Users/me/.ssh/id_rsa",
            "127.0.0.1:7891:127.0.0.1:7890",
            "ubuntu@example.com",
        );

        assert!(
            args.windows(2)
                .any(|pair| pair == ["-o", "ServerAliveInterval=30"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-o", "ServerAliveCountMax=3"])
        );
    }

    #[test]
    fn wait_for_forward_allows_slow_ssh_startup() {
        if Command::new("python3")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_err()
        {
            eprintln!("skipping test because python3 is not available");
            return;
        }

        let port = unused_local_port();
        let script = format!(
            r#"
import socket
import time

time.sleep(3)
listener = socket.socket()
listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
listener.bind(("127.0.0.1", {port}))
listener.listen(1)
time.sleep(2)
"#
        );

        let mut child = Command::new("python3")
            .arg("-c")
            .arg(script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let started = Instant::now();
        let result = wait_for_forward(&mut child, "127.0.0.1", port);
        let _ = child.kill();
        let _ = child.wait();

        assert!(
            result.is_ok(),
            "expected delayed listener to become ready, got {result:?}"
        );
        assert!(started.elapsed() >= Duration::from_secs(3));
    }

    fn unused_local_port() -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    }
}
