use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

/// Start monitoring multiple files. All lines go into one channel.
pub async fn start_multi_file_source(
    paths: Vec<PathBuf>,
) -> Result<(mpsc::UnboundedReceiver<String>, String), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::unbounded_channel();

    let mut names: Vec<String> = Vec::new();
    let mut mux = linemux::MuxedLines::new()?;

    for path in &paths {
        let path = path.canonicalize().unwrap_or(path.clone());
        if !path.exists() {
            return Err(format!("file not found: {}", path.display()).into());
        }

        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        names.push(name);

        // Send existing content
        let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        for line in content.lines() {
            let _ = tx.send(line.to_string());
        }

        mux.add_file(&path).await?;
    }

    let display_name = if names.len() == 1 {
        names[0].clone()
    } else {
        format!("{} files ({})", names.len(), names.join(", "))
    };

    tokio::spawn(async move {
        loop {
            match mux.next_line().await {
                Ok(Some(line)) => {
                    if tx.send(line.line().to_string()).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    });

    Ok((rx, display_name))
}

pub async fn start_stdin_source(
) -> Result<(mpsc::UnboundedReceiver<String>, String), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    Ok((rx, "stdin".to_string()))
}

pub async fn start_docker_source(
    container: String,
    file_path: Option<String>,
) -> Result<(mpsc::UnboundedReceiver<String>, String), Box<dyn std::error::Error>> {
    // Check that docker is available
    let docker_check = tokio::process::Command::new("docker")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    if !docker_check.success() {
        return Err("docker is not available or not running".into());
    }

    // Check container exists and is running
    let inspect = tokio::process::Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", &container])
        .output()
        .await?;

    let running = String::from_utf8_lossy(&inspect.stdout).trim().to_string();
    if running != "true" {
        return Err(format!("container '{}' is not running", container).into());
    }

    let (tx, rx) = mpsc::unbounded_channel();

    let name = match &file_path {
        Some(fp) => format!("{}:{}", container, fp),
        None => format!("{} (stdout)", container),
    };

    match file_path {
        Some(fp) => {
            // Stream file from inside container using docker exec
            // First, detect available shell
            let shell = detect_container_shell(&container).await;

            let mut child = tokio::process::Command::new("docker")
                .args([
                    "exec",
                    &container,
                    &shell,
                    "-c",
                    &format!("tail -n +1 -f {}", fp),
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;

            let stdout = child.stdout.take().expect("stdout must be piped");
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                let _ = child.wait().await;
            });
        }
        None => {
            // Use docker logs -f
            let mut child = tokio::process::Command::new("docker")
                .args(["logs", "-f", "--tail", "1000", &container])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;

            let stdout = child.stdout.take().expect("stdout must be piped");
            let stderr = child.stderr.take().expect("stderr must be piped");

            // Docker logs sends stdout and stderr separately
            // Read both
            let tx2 = tx.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });

            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if tx2.send(line).is_err() {
                        break;
                    }
                }
                let _ = child.wait().await;
            });
        }
    }

    Ok((rx, name))
}

async fn detect_container_shell(container: &str) -> String {
    // Try bash first, then sh
    for shell in ["bash", "sh"] {
        let result = tokio::process::Command::new("docker")
            .args(["exec", container, "which", shell])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;

        if let Ok(status) = result {
            if status.success() {
                return shell.to_string();
            }
        }
    }
    "sh".to_string()
}
