mod clash;
mod config;
mod error;
mod state;
mod tunnel;
mod utils;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Config;
use error::ResipError;

#[derive(Parser)]
#[command(name = "resip")]
#[command(version)]
#[command(about = "Manage a local Clash to remote Mihomo/Clash SSH tunnel")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactively initialize configuration.
    Init {
        /// Server IP or hostname.
        server_ip: Option<String>,
        /// Overwrite an existing configuration.
        #[arg(short, long)]
        force: bool,
        /// Clash YAML output path.
        #[arg(long)]
        output: Option<String>,
    },
    /// Generate local Clash YAML.
    Gen {
        /// Open the generated config directory after writing the YAML.
        #[arg(long)]
        open: bool,
    },
    /// Start the SSH tunnel in the background.
    On {
        /// Stop an existing tunnel before starting a new one.
        #[arg(short, long)]
        force: bool,
    },
    /// Stop the SSH tunnel.
    Off,
    /// Show configuration and tunnel status.
    Status,
    /// Test the current proxy chain through local Clash.
    Test,
    /// Print the SSH tunnel command for manual debugging.
    Ssh,
    /// Open the generated Clash config directory.
    Open,
    /// First-time setup: init + gen.
    Setup {
        /// Server IP or hostname.
        server_ip: Option<String>,
        /// Overwrite an existing configuration.
        #[arg(short, long)]
        force: bool,
        /// Clash YAML output path.
        #[arg(long)]
        output: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            server_ip,
            force,
            output,
        } => init(server_ip, force, output),
        Commands::Gen { open } => {
            let config = Config::load()?;
            let path = clash::generate(&config)?;
            println!("Generated Clash config:");
            println!("File: {}", path.display());
            println!();
            if open {
                let dir = utils::open_path_dir(&path)
                    .map_err(|error| ResipError::GeneratedButOpenFailed(error.into()))?;
                print_opened_clash_config_dir(&dir);
                println!();
            }
            print_next_steps();
            Ok(())
        }
        Commands::On { force } => {
            let config = Config::load()?;
            tunnel::start(&config, force)?;
            Ok(())
        }
        Commands::Off => {
            tunnel::stop()?;
            Ok(())
        }
        Commands::Status => status(),
        Commands::Test => test(),
        Commands::Ssh => {
            let config = Config::load()?;
            println!("{}", tunnel::ssh_command_string(&config));
            Ok(())
        }
        Commands::Open => open_clash_config_dir(),
        Commands::Setup {
            server_ip,
            force,
            output,
        } => {
            init(server_ip, force, output)?;
            let config = Config::load()?;
            let path = clash::generate(&config)?;
            print_generated_clash_config(&path);
            Ok(())
        }
    }
}

fn init(server_ip: Option<String>, force: bool, output: Option<String>) -> Result<()> {
    let path = Config::path()?;

    if path.exists() && !force {
        let overwrite = utils::prompt_yes_no(
            &format!(
                "Configuration already exists at {}. Overwrite?",
                path.display()
            ),
            false,
        )?;
        if !overwrite {
            println!("Kept existing configuration: {}", path.display());
            return Ok(());
        }
    }

    let config = Config::interactive(server_ip, output)?;
    config.save()?;
    println!("Saved configuration: {}", path.display());
    Ok(())
}

fn print_generated_clash_config(path: &std::path::Path) {
    println!("Generated Clash config:");
    println!("File: {}", path.display());
    println!();
    print_next_steps();
}

fn print_opened_clash_config_dir(dir: &std::path::Path) {
    println!("Opened Clash config directory:");
    println!("Directory: {}", dir.display());
}

fn print_next_steps() {
    println!("Next steps:");
    println!("1. Import this YAML file into Clash.");
    println!("2. Run `resip on`.");
    println!("3. Enable Clash system proxy.");
    println!();
    println!("Run `resip open` to open the config directory.");
}

fn open_clash_config_dir() -> Result<()> {
    let config = Config::load()?;
    let output_path = utils::expand_tilde(&config.clash_output_path)?;
    let opened_dir = utils::open_path_dir(&output_path)?;
    print_opened_clash_config_dir(&opened_dir);
    Ok(())
}

fn status() -> Result<()> {
    let config_path = Config::path()?;
    let state_path = state::State::path()?;

    println!("Config: {}", config_path.display());
    println!("State: {}", state_path.display());

    let config = if config_path.exists() {
        Some(Config::load()?)
    } else {
        None
    };

    if let Some(current) = state::State::load_optional()? {
        let running = tunnel::is_pid_running(current.pid);
        println!("Tunnel: {}", if running { "running" } else { "stale" });
        println!("PID: {}", current.pid);
        println!("Started at: {}", current.started_at);
        if let Some(config) = &config {
            tunnel::print_tunnel_details(config, None);
            println!("Clash config: {}", config.clash_output_path);
        } else {
            println!(
                "Local: {}:{} on this machine",
                current.local_tunnel_host, current.local_tunnel_port
            );
            println!("SSH: {}", current.server);
            println!("Clash config: not configured");
        }
    } else {
        println!("Tunnel: stopped");
        if let Some(config) = &config {
            tunnel::print_tunnel_details(config, None);
            println!("Clash config: {}", config.clash_output_path);
        } else {
            println!("SSH: not configured");
            println!("Clash config: not configured");
        }
    }

    Ok(())
}

fn test() -> Result<()> {
    let config = Config::load()?;
    let proxy_url = format!("http://127.0.0.1:{}", config.local_clash_port);
    let proxy = reqwest::Proxy::http(&proxy_url).map_err(|source| ResipError::CreateHttpProxy {
        url: proxy_url.clone(),
        source,
    })?;
    let client = reqwest::blocking::Client::builder()
        .proxy(proxy)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(ResipError::BuildHttpClient)?;

    let result = client
        .get("https://ipinfo.io/json")
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .and_then(|response| response.json::<serde_json::Value>());

    match result {
        Ok(json) => {
            for key in ["ip", "city", "region", "country", "org"] {
                let value = match json.get(key).and_then(serde_json::Value::as_str) {
                    Some(value) => value,
                    None => "-",
                };
                println!("{key}: {value}");
            }
            Ok(())
        }
        Err(error) => {
            eprintln!("Proxy test failed: {error}");
            eprintln!("Check:");
            eprintln!("- `resip on` has started the SSH tunnel");
            eprintln!("- Clash imported the YAML generated by `resip gen`");
            eprintln!("- Clash system proxy is enabled");
            Err(ResipError::ProxyTest(error).into())
        }
    }
}
