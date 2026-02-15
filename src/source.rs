use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

const TAIL_LINES: usize = 1000;
/// Chunk size for seeking backwards through large files.
const TAIL_CHUNK: u64 = 64 * 1024;

/// Read the last `n` lines from a file by seeking from the end.
/// For small files (< TAIL_CHUNK), reads the whole thing.
/// For large files, reads backwards in chunks until enough newlines are found.
fn read_tail(path: &std::path::Path, n: usize) -> std::io::Result<Vec<String>> {
    let mut file = std::fs::File::open(path)?;
    let file_len = file.metadata()?.len();

    if file_len == 0 {
        return Ok(Vec::new());
    }

    // Small file — just read it all
    if file_len <= TAIL_CHUNK {
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        let lines: Vec<String> = buf.lines().map(String::from).collect();
        let skip = lines.len().saturating_sub(n);
        return Ok(lines.into_iter().skip(skip).collect());
    }

    // Large file — seek backwards in chunks
    let mut collected = Vec::new();
    let mut pos = file_len;

    loop {
        let read_start = pos.saturating_sub(TAIL_CHUNK);
        let read_len = (pos - read_start) as usize;
        file.seek(SeekFrom::Start(read_start))?;

        let mut buf = vec![0u8; read_len];
        file.read_exact(&mut buf)?;

        let chunk = String::from_utf8_lossy(&buf);
        let mut chunk_lines: Vec<String> = chunk.lines().map(String::from).collect();

        // If we're not at the start, the first line is likely partial — remove it
        if read_start > 0 && !chunk_lines.is_empty() {
            chunk_lines.remove(0);
        }

        chunk_lines.append(&mut collected);
        collected = chunk_lines;

        if collected.len() >= n || read_start == 0 {
            break;
        }
        pos = read_start;
    }

    let skip = collected.len().saturating_sub(n);
    Ok(collected.into_iter().skip(skip).collect())
}

// ---------------------------------------------------------------------------
// Multi-file source (local)
// ---------------------------------------------------------------------------

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

        // Read only the last TAIL_LINES lines (like tail -n 1000) to avoid
        // loading huge files into memory and blocking the TUI startup.
        if let Ok(lines) = read_tail(&path, TAIL_LINES) {
            for line in lines {
                let _ = tx.send(line);
            }
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

// ---------------------------------------------------------------------------
// Stdin source
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Docker source — smart prefix match + auto-reconnect
// ---------------------------------------------------------------------------

/// Find a running container whose name starts with `prefix`.
async fn find_container_by_prefix(prefix: &str) -> Option<String> {
    let output = tokio::process::Command::new("docker")
        .args([
            "ps",
            "--format",
            "{{.Names}}",
            "--filter",
            &format!("name={}", prefix),
            "--filter",
            "status=running",
        ])
        .output()
        .await
        .ok()?;

    let names = String::from_utf8_lossy(&output.stdout);
    // Return the first match (most relevant)
    names.lines().next().map(|s| s.to_string())
}

/// Find a running container whose name starts with `prefix`, via SSH.
async fn find_container_by_prefix_ssh(opts: &SshOpts, prefix: &str) -> Option<String> {
    let mut args = ssh_base_args(opts);
    args.extend([
        "docker".to_string(),
        "ps".to_string(),
        "--format".to_string(),
        "{{.Names}}".to_string(),
        "--filter".to_string(),
        format!("name={}", prefix),
        "--filter".to_string(),
        "status=running".to_string(),
    ]);

    let output = tokio::process::Command::new("ssh")
        .args(&args)
        .output()
        .await
        .ok()?;

    let names = String::from_utf8_lossy(&output.stdout);
    names.lines().next().map(|s| s.to_string())
}

/// Stream docker logs from a specific container. Returns the child process.
fn spawn_docker_logs(
    container: &str,
    file_path: Option<&str>,
) -> std::io::Result<tokio::process::Child> {
    match file_path {
        Some(fp) => tokio::process::Command::new("docker")
            .args([
                "exec",
                container,
                "sh",
                "-c",
                &format!("tail -n +1 -f {}", fp),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn(),
        None => tokio::process::Command::new("docker")
            .args(["logs", "-f", "--tail", "1000", container])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn(),
    }
}

/// Stream docker logs from a container via SSH.
fn spawn_docker_logs_ssh(
    opts: &SshOpts,
    container: &str,
    file_path: Option<&str>,
) -> std::io::Result<tokio::process::Child> {
    let docker_cmd = match file_path {
        Some(fp) => format!("docker exec {} sh -c 'tail -n +1 -f {}'", container, fp),
        None => format!("docker logs -f --tail 1000 {}", container),
    };
    let mut args = ssh_base_args(opts);
    args.push(docker_cmd);
    tokio::process::Command::new("ssh")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

/// Pipe all lines from a child process stdout (and optionally stderr) into tx.
/// Returns when the child exits.
async fn pipe_child_to_tx(
    mut child: tokio::process::Child,
    tx: &mpsc::UnboundedSender<String>,
    capture_stderr: bool,
) {
    if let Some(stdout) = child.stdout.take() {
        let tx_out = tx.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_out.send(line).is_err() {
                    break;
                }
            }
        });
    }

    if capture_stderr {
        if let Some(stderr) = child.stderr.take() {
            let tx_err = tx.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if tx_err.send(line).is_err() {
                        break;
                    }
                }
            });
        }
    }

    let _ = child.wait().await;
}

pub async fn start_docker_source(
    prefix: String,
    file_path: Option<String>,
) -> Result<(mpsc::UnboundedReceiver<String>, String), Box<dyn std::error::Error>> {
    // Find container by prefix
    let container = find_container_by_prefix(&prefix)
        .await
        .ok_or_else(|| format!("no running container matching '{}'", prefix))?;

    let display_name = match &file_path {
        Some(fp) => format!("{}:{}", prefix, fp),
        None => format!("{} ({})", prefix, &container[..container.len().min(20)]),
    };

    let (tx, rx) = mpsc::unbounded_channel();
    let _ = tx.send(format!(">>> connected to container: {}", container));

    let fp = file_path.clone();
    let prefix_owned = prefix.clone();
    tokio::spawn(async move {
        let mut current_container = container;
        loop {
            let child = spawn_docker_logs(&current_container, fp.as_deref());
            if let Ok(child) = child {
                pipe_child_to_tx(child, &tx, fp.is_none()).await;
            }

            // Container died — try to reconnect
            if tx
                .send(">>> container stopped, reconnecting...".to_string())
                .is_err()
            {
                break;
            }

            let mut reconnected = false;
            for _ in 0..150 {
                // Try for 5 minutes
                sleep(Duration::from_secs(2)).await;
                if let Some(new_container) = find_container_by_prefix(&prefix_owned).await {
                    let _ = tx.send(format!(">>> reconnected to container: {}", new_container));
                    current_container = new_container;
                    reconnected = true;
                    break;
                }
            }
            if !reconnected {
                let _ = tx.send(">>> gave up reconnecting after 5 minutes".to_string());
                break;
            }
        }
    });

    Ok((rx, display_name))
}

// ---------------------------------------------------------------------------
// SSH helpers
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct SshOpts {
    pub target: String,
    pub port: Option<u16>,
    pub key: Option<String>,
    pub jump: Option<String>,
}

/// Build the base ssh args: [-p port] [-i key] [-J jump] target
fn ssh_base_args(opts: &SshOpts) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(port) = opts.port {
        args.push("-p".to_string());
        args.push(port.to_string());
    }
    if let Some(ref key) = opts.key {
        args.push("-i".to_string());
        args.push(key.clone());
    }
    if let Some(ref jump) = opts.jump {
        args.push("-J".to_string());
        args.push(jump.clone());
    }
    args.push(opts.target.clone());
    args
}

// ---------------------------------------------------------------------------
// SSH source — run any command remotely
// ---------------------------------------------------------------------------

pub async fn start_ssh_file_source(
    opts: SshOpts,
    file_path: String,
) -> Result<(mpsc::UnboundedReceiver<String>, String), Box<dyn std::error::Error>> {
    let display_name = format!("{}:{}", opts.target, file_path);
    let (tx, rx) = mpsc::unbounded_channel();

    let mut args = ssh_base_args(&opts);
    args.push(format!("tail -n 1000 -f {}", file_path));

    let mut child = tokio::process::Command::new("ssh")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout piped");
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

    Ok((rx, display_name))
}

pub async fn start_ssh_docker_source(
    opts: SshOpts,
    prefix: String,
    file_path: Option<String>,
) -> Result<(mpsc::UnboundedReceiver<String>, String), Box<dyn std::error::Error>> {
    let container = find_container_by_prefix_ssh(&opts, &prefix)
        .await
        .ok_or_else(|| {
            format!(
                "no running container matching '{}' on {}",
                prefix, opts.target
            )
        })?;

    let display_name = match &file_path {
        Some(fp) => format!("ssh://{}:{}:{}", opts.target, prefix, fp),
        None => format!("ssh://{}:{}", opts.target, prefix),
    };

    let (tx, rx) = mpsc::unbounded_channel();
    let _ = tx.send(format!(
        ">>> connected via ssh to {}:{}",
        opts.target, container
    ));

    let fp = file_path.clone();
    let prefix_owned = prefix.clone();
    tokio::spawn(async move {
        let mut current_container = container;
        loop {
            let child = spawn_docker_logs_ssh(&opts, &current_container, fp.as_deref());
            if let Ok(child) = child {
                pipe_child_to_tx(child, &tx, fp.is_none()).await;
            }

            if tx
                .send(">>> container stopped, reconnecting...".to_string())
                .is_err()
            {
                break;
            }

            let mut reconnected = false;
            for _ in 0..150 {
                sleep(Duration::from_secs(2)).await;
                if let Some(new_c) = find_container_by_prefix_ssh(&opts, &prefix_owned).await {
                    let _ = tx.send(format!(">>> reconnected to {}:{}", opts.target, new_c));
                    current_container = new_c;
                    reconnected = true;
                    break;
                }
            }
            if !reconnected {
                let _ = tx.send(">>> gave up reconnecting after 5 minutes".to_string());
                break;
            }
        }
    });

    Ok((rx, display_name))
}

// ---------------------------------------------------------------------------
// Kubernetes source
// ---------------------------------------------------------------------------

/// Find a pod by label selector in a namespace.
async fn find_pod_by_label(namespace: &str, label: &str) -> Option<String> {
    let output = tokio::process::Command::new("kubectl")
        .args([
            "get",
            "pods",
            "-n",
            namespace,
            "-l",
            label,
            "--field-selector=status.phase=Running",
            "-o",
            "jsonpath={.items[0].metadata.name}",
        ])
        .output()
        .await
        .ok()?;

    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

pub async fn start_k8s_source(
    pod: Option<String>,
    namespace: String,
    container: Option<String>,
    label: Option<String>,
    file_path: Option<String>,
) -> Result<(mpsc::UnboundedReceiver<String>, String), Box<dyn std::error::Error>> {
    // Resolve pod name
    let pod_name = if let Some(p) = pod {
        p
    } else if let Some(lbl) = &label {
        find_pod_by_label(&namespace, lbl).await.ok_or_else(|| {
            format!(
                "no running pod matching label '{}' in namespace '{}'",
                lbl, namespace
            )
        })?
    } else {
        return Err("either pod name or --label is required".into());
    };

    let display_name = match &file_path {
        Some(fp) => format!("k8s:{}/{}:{}", namespace, pod_name, fp),
        None => format!("k8s:{}/{}", namespace, pod_name),
    };

    let (tx, rx) = mpsc::unbounded_channel();

    match file_path {
        Some(fp) => {
            // Read file inside pod via kubectl exec
            let mut args = vec!["exec".to_string(), pod_name, "-n".to_string(), namespace];
            if let Some(c) = container {
                args.push("-c".to_string());
                args.push(c);
            }
            args.push("--".to_string());
            args.push("sh".to_string());
            args.push("-c".to_string());
            args.push(format!("tail -n +1 -f {}", fp));

            let mut child = tokio::process::Command::new("kubectl")
                .args(&args)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;

            let stdout = child.stdout.take().expect("stdout piped");
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
            // kubectl logs -f
            let mut args = vec![
                "logs".to_string(),
                "-f".to_string(),
                "--tail=1000".to_string(),
                pod_name,
                "-n".to_string(),
                namespace,
            ];
            if let Some(c) = container {
                args.push("-c".to_string());
                args.push(c);
            }

            let mut child = tokio::process::Command::new("kubectl")
                .args(&args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;

            let stdout = child.stdout.take().expect("stdout piped");
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

            if let Some(stderr) = child.stderr.take() {
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
    }

    Ok((rx, display_name))
}

// ---------------------------------------------------------------------------
// Docker Compose source
// ---------------------------------------------------------------------------

pub async fn start_compose_source(
    service: String,
    compose_file: Option<String>,
) -> Result<(mpsc::UnboundedReceiver<String>, String), Box<dyn std::error::Error>> {
    let display_name = format!("compose:{}", service);
    let (tx, rx) = mpsc::unbounded_channel();

    let mut args = vec!["compose".to_string()];
    if let Some(f) = compose_file {
        args.push("-f".to_string());
        args.push(f);
    }
    args.extend(["logs", "-f", "--tail", "1000", "--no-log-prefix"].map(String::from));
    args.push(service);

    let mut child = tokio::process::Command::new("docker")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

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

    Ok((rx, display_name))
}
