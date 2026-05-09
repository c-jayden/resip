use resip::config::Config;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn config_saves_and_loads_from_explicit_path() {
    let temp = temp_dir("config-save-load");
    let path = temp.join("nested").join("config.json");
    let config = Config {
        ssh_host: "203.0.113.10".to_string(),
        ssh_user: "admin".to_string(),
        ssh_port: 2222,
        clash_output_path: temp.join("resip.yaml").display().to_string(),
        ..Config::default()
    };

    config.save_to_path(&path).unwrap();
    let loaded = Config::load_from_path(&path).unwrap();

    assert_eq!(loaded.ssh_host, "203.0.113.10");
    assert_eq!(loaded.ssh_user, "admin");
    assert_eq!(loaded.ssh_port, 2222);
    assert_eq!(loaded.clash_output_path, config.clash_output_path);
}

#[test]
fn init_then_gen_writes_expected_clash_yaml() {
    let temp = temp_dir("init-gen");
    let output_path = temp.join("clash.yaml");

    // Use an isolated HOME so the binary does not read or write real config.
    let init = command_with_home(&temp)
        .args([
            "init",
            "203.0.113.10",
            "--output",
            output_path.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut init = init;
    {
        use std::io::Write;
        let stdin = init.stdin.as_mut().unwrap();
        stdin.write_all(b"\n\n\n").unwrap();
    }
    let init_output = init.wait_with_output().unwrap();
    assert!(
        init_output.status.success(),
        "init failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&init_output.stdout),
        String::from_utf8_lossy(&init_output.stderr)
    );

    let gen_output = command_with_home(&temp).arg("gen").output().unwrap();
    assert!(
        gen_output.status.success(),
        "gen failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&gen_output.stdout),
        String::from_utf8_lossy(&gen_output.stderr)
    );

    let yaml = fs::read_to_string(&output_path).unwrap();
    let value: serde_yml::Value = serde_yml::from_str(&yaml).unwrap();

    assert_eq!(value["port"].as_u64(), Some(7890));
    assert_eq!(value["allow-lan"].as_bool(), Some(false));
    assert_eq!(value["proxies"][0]["name"].as_str(), Some("resip-server"));
    assert_eq!(value["proxies"][0]["server"].as_str(), Some("127.0.0.1"));
    assert_eq!(value["proxies"][0]["port"].as_u64(), Some(7891));
    assert_eq!(value["rules"][0].as_str(), Some("MATCH,RESIP"));
}

#[test]
fn help_lists_primary_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_resip"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: resip <COMMAND>"));
    assert!(stdout.contains("init"));
    assert!(stdout.contains("gen"));
    assert!(stdout.contains("status"));
}

fn command_with_home(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_resip"));
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local").join("share"));
    command
}

fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("resip-{name}-{nanos}"));
    fs::create_dir_all(&path).unwrap();
    path
}
