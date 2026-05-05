//! `nostube-transcode docker` subcommands — Docker lifecycle management.

use anyhow::{bail, Result};
use std::process::Command;

/// Detect the docker compose command available on this system.
/// Returns "docker" with args ["compose", ...] or "docker-compose" as fallback.
fn compose_cmd() -> (&'static str, &'static [&'static str]) {
    // Prefer `docker compose` (V2 plugin)
    let v2_ok = Command::new("docker")
        .args(["compose", "version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if v2_ok {
        ("docker", &["compose"])
    } else {
        ("docker-compose", &[])
    }
}

/// Check that docker (or docker-compose) is available.
fn require_docker() -> Result<()> {
    let ok = Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        bail!(
            "Docker is not running or not installed.\n\
             Install Docker Desktop: https://www.docker.com/products/docker-desktop/"
        );
    }
    Ok(())
}

/// Run setup.sh — detect GPU, write .env, start compose.
pub fn setup() -> Result<()> {
    require_docker()?;

    // setup.sh must be in the repo root (CWD for Docker deployments)
    if !std::path::Path::new("setup.sh").exists() {
        bail!(
            "setup.sh not found in the current directory.\n\
             Clone the repository and run this command from its root:\n\
             git clone https://github.com/flox1an/nostube-transcode.git\n\
             cd nostube-transcode && nostube-transcode docker setup"
        );
    }

    let status = Command::new("bash").arg("setup.sh").status()?;
    if !status.success() {
        bail!("setup.sh exited with status {}", status);
    }
    Ok(())
}

/// `docker compose ps` — show running containers.
pub fn docker_status() -> Result<()> {
    require_docker()?;
    let (cmd, prefix) = compose_cmd();
    let mut args: Vec<&str> = prefix.to_vec();
    args.push("ps");

    let status = Command::new(cmd).args(&args).status()?;
    if !status.success() {
        bail!("docker compose ps failed");
    }
    Ok(())
}

/// `docker compose logs` — follow or print recent logs.
pub fn logs(follow: bool) -> Result<()> {
    require_docker()?;
    let (cmd, prefix) = compose_cmd();
    let mut args: Vec<&str> = prefix.to_vec();
    args.push("logs");
    if follow {
        args.push("-f");
    } else {
        args.extend_from_slice(&["--tail", "100"]);
    }

    let status = Command::new(cmd).args(&args).status()?;
    if !status.success() {
        bail!("docker compose logs failed");
    }
    Ok(())
}

/// `docker compose up -d` — start the stack.
pub fn start() -> Result<()> {
    require_docker()?;
    let (cmd, prefix) = compose_cmd();
    let mut args: Vec<&str> = prefix.to_vec();
    args.extend_from_slice(&["up", "-d"]);

    let status = Command::new(cmd).args(&args).status()?;
    if !status.success() {
        bail!("docker compose up -d failed");
    }
    println!("Docker stack started. Admin UI: http://localhost:5207");
    Ok(())
}

/// `docker compose down` — stop the stack.
pub fn stop() -> Result<()> {
    require_docker()?;
    let (cmd, prefix) = compose_cmd();
    let mut args: Vec<&str> = prefix.to_vec();
    args.push("down");

    let status = Command::new(cmd).args(&args).status()?;
    if !status.success() {
        bail!("docker compose down failed");
    }
    println!("Docker stack stopped.");
    Ok(())
}

/// `docker compose restart` — restart all containers.
pub fn restart() -> Result<()> {
    require_docker()?;
    let (cmd, prefix) = compose_cmd();
    let mut args: Vec<&str> = prefix.to_vec();
    args.push("restart");

    let status = Command::new(cmd).args(&args).status()?;
    if !status.success() {
        bail!("docker compose restart failed");
    }
    println!("Docker stack restarted.");
    Ok(())
}
