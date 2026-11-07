//! Self-contained Lingua Tutor CLI.

use lingua_tutor_local::{assets, server, storage, tunnel};
#[cfg(unix)]
use std::{fs::OpenOptions, io::Read};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, ExitCode, Stdio},
};

const STATE_DIR: &str = ".lingua-tutor";
const DB_NAME: &str = "lingua.sqlite3";
const WEB_DIR: &str = "web";

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let action = args.next().unwrap_or_else(|| "run".to_owned());

    match action.as_str() {
        "run" => run_project(args.collect()).await,
        "check" => {
            let paths = ProjectPaths::from_current_dir()?;
            check_project(&paths)
        }
        "backup" => {
            let paths = ProjectPaths::from_current_dir()?;
            ensure_initialized(&paths)?;
            println!("{}", storage::backup(&paths.db)?.display());
            Ok(())
        }
        "where" => {
            let paths = ProjectPaths::from_current_dir()?;
            println!("project: {}", paths.root.display());
            println!("state:   {}", paths.state.display());
            println!("db:      {}", paths.db.display());
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!(
            "unknown command: {other}\n\nRun lingua-tutor help for usage."
        )),
    }
}

async fn run_project(args: Vec<String>) -> Result<(), String> {
    let mut port = 8787_u16;
    let mut no_tunnel = false;
    let mut iter = args.into_iter();
    while let Some(option) = iter.next() {
        match option.as_str() {
            "--port" => {
                let value = iter.next().ok_or("--port requires a value")?;
                port = value.parse().map_err(|_| "invalid port")?;
            }
            "--no-tunnel" => no_tunnel = true,
            _ => return Err(format!("unknown run option: {option}")),
        }
    }

    let paths = ProjectPaths::from_current_dir()?;
    let first_run = !paths.db.is_file();
    ensure_initialized(&paths)?;
    if !first_run {
        storage::backup_if_due(&paths.db)?;
    }
    let env = storage::environment(&paths.db)?;
    println!("Lingua Tutor");
    println!("  project: {}", paths.root.display());
    println!("  database: {}", paths.db.display());
    println!("  UI: http://127.0.0.1:{port}");
    if first_run {
        println!("  initialized new project state");
    } else {
        println!("  using existing project state");
    }

    if no_tunnel {
        println!("  tunnel: disabled for this session");
        return server::serve(env, &paths.web, port).await;
    }

    let tunnel_id = prompt_secret("Tunnel ID: ")?;
    let api_key = prompt_secret("Runtime API key: ")?;
    let config = tunnel::TunnelConfig::production(tunnel_id, api_key)?;

    tokio::select! {
        result = server::serve(env.clone(), &paths.web, port) => result,
        result = tunnel::run(env, config) => result,
    }
}

fn ensure_initialized(paths: &ProjectPaths) -> Result<(), String> {
    storage::open(&paths.db)?;
    assets::write_to(&paths.web)?;
    Ok(())
}

fn check_project(paths: &ProjectPaths) -> Result<(), String> {
    if !paths.db.is_file() {
        return Err(format!(
            "no Lingua Tutor project in {}\nRun lingua-tutor run first.",
            paths.root.display()
        ));
    }
    storage::open(&paths.db)?;
    if !paths.web.join("index.html").is_file() {
        return Err(format!("web assets are missing: {}", paths.web.display()));
    }
    println!("project ok");
    println!("database: {}", paths.db.display());
    Ok(())
}

#[derive(Debug)]
struct ProjectPaths {
    root: PathBuf,
    state: PathBuf,
    db: PathBuf,
    web: PathBuf,
}

impl ProjectPaths {
    fn from_current_dir() -> Result<Self, String> {
        let root = std::env::current_dir().map_err(|error| error.to_string())?;
        let state = root.join(STATE_DIR);
        Ok(Self {
            db: state.join(DB_NAME),
            web: state.join(WEB_DIR),
            state,
            root,
        })
    }
}

fn prompt_secret(label: &str) -> Result<String, String> {
    #[cfg(unix)]
    {
        prompt_secret_unix(label)
    }
    #[cfg(windows)]
    {
        prompt_secret_windows(label)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = label;
        Err("secure terminal input is unsupported on this platform".to_owned())
    }
}

#[cfg(unix)]
fn set_terminal_echo(tty: &std::fs::File, enabled: bool) -> Result<(), String> {
    let input = tty.try_clone().map_err(|error| error.to_string())?;
    let status = Command::new("stty")
        .arg(if enabled { "echo" } else { "-echo" })
        .stdin(Stdio::from(input))
        .status()
        .map_err(|error| format!("failed to run stty: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("stty failed to update terminal echo".to_owned())
    }
}

#[cfg(unix)]
fn prompt_secret_unix(label: &str) -> Result<String, String> {
    let mut tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|error| format!("cannot open controlling terminal: {error}"))?;
    tty.write_all(label.as_bytes())
        .and_then(|_| tty.flush())
        .map_err(|error| error.to_string())?;

    set_terminal_echo(&tty, false)?;

    struct RestoreEcho(std::fs::File);
    impl Drop for RestoreEcho {
        fn drop(&mut self) {
            let _ = set_terminal_echo(&self.0, true);
        }
    }
    let _restore = RestoreEcho(tty.try_clone().map_err(|error| error.to_string())?);

    let mut bytes = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        let read = tty.read(&mut byte).map_err(|error| error.to_string())?;
        if read == 0 || matches!(byte[0], b'\n' | b'\r') {
            break;
        }
        bytes.push(byte[0]);
        if bytes.len() > 16 * 1024 {
            bytes.fill(0);
            return Err("secret input is too long".to_owned());
        }
    }
    tty.write_all(b"\n").map_err(|error| error.to_string())?;
    let value = String::from_utf8(bytes).map_err(|_| "input is not valid UTF-8".to_owned())?;
    if value.trim().is_empty() {
        return Err("value cannot be empty".to_owned());
    }
    Ok(value)
}

#[cfg(windows)]
fn prompt_secret_windows(label: &str) -> Result<String, String> {
    const SCRIPT: &str = r#"
$secure = Read-Host -Prompt $env:LINGUA_TUTOR_PROMPT -AsSecureString
$pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
try {
    [Console]::Out.Write([Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer))
}
finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
}
"#;

    let output = Command::new("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-Command", SCRIPT])
        .env("LINGUA_TUTOR_PROMPT", label.trim_end())
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("failed to start Windows secure prompt: {error}"))?;
    if !output.status.success() {
        return Err("Windows secure prompt failed".to_owned());
    }
    let value =
        String::from_utf8(output.stdout).map_err(|_| "input is not valid UTF-8".to_owned())?;
    if value.trim().is_empty() {
        return Err("value cannot be empty".to_owned());
    }
    Ok(value)
}

fn print_help() {
    println!(
        "Lingua Tutor\n\
         \n\
         Usage:\n\
           lingua-tutor run [--port N] [--no-tunnel]\n\
           lingua-tutor check\n\
           lingua-tutor backup\n\
           lingua-tutor where\n\
         \n\
         Project state:\n\
           The current directory is the project. Persistent state lives only in\n\
           .lingua-tutor/ (SQLite + web assets). Tunnel ID and Runtime API key are\n\
           requested with hidden terminal input on every run and are never saved.\n\
         \n\
         Examples:\n\
           mkdir ~/spanish && cd ~/spanish\n\
           lingua-tutor run\n\
         \n\
           # Local UI only, no tunnel:\n\
           lingua-tutor run --no-tunnel"
    );
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn project_state_is_directory_local() {
        let root = Path::new("/tmp/study");
        let state = root.join(STATE_DIR);
        assert_eq!(
            state.join(DB_NAME),
            root.join(".lingua-tutor/lingua.sqlite3")
        );
        assert_eq!(state.join(WEB_DIR), root.join(".lingua-tutor/web"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_secure_prompt_backend_is_available() {
        let status = Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-Command", "exit 0"])
            .status()
            .unwrap();
        assert!(status.success());
    }
}
