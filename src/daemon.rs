use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::future;
use std::sync::Arc;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    process::Command,
    sync::Mutex,
};

// AppState to hold all shared state
#[derive(Clone)]
struct AppState {
    sticky_windows: Arc<Mutex<HashSet<u64>>>,
    staged_set: Arc<Mutex<HashSet<u64>>>,
}

impl AppState {
    fn new(sticky_windows: Arc<Mutex<HashSet<u64>>>) -> Self {
        Self {
            sticky_windows,
            staged_set: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

pub async fn start(sticky_windows: Arc<Mutex<HashSet<u64>>>) -> Result<()> {
    let app_state = AppState::new(sticky_windows);

    let state_clone_cli = app_state.clone();
    tokio::spawn(async move {
        if let Err(_e) = run_cli_server(state_clone_cli).await {
            eprintln!("CLI server error: {{_e:?}}");
        }
    });

    let state_clone_watcher = app_state.clone();
    tokio::spawn(async move {
        if let Err(_e) = run_watcher(state_clone_watcher).await {
            eprintln!("Watcher error: {{_e:?}}");
        }
    });

    println!("nsticky daemon started.");
    future::pending::<()>().await;
    Ok(())
}

async fn run_cli_server(app_state: AppState) -> Result<()> {
    let cli_socket_path = "/tmp/niri_sticky_cli.sock";
    let _ = std::fs::remove_file(cli_socket_path);
    let listener = UnixListener::bind(cli_socket_path)?;

    loop {
        let (stream, _) = listener.accept().await?;
        let state_clone = app_state.clone();
        tokio::spawn(async move {
            if let Err(_e) = handle_cli_connection(stream, state_clone).await {
                eprintln!("CLI connection error: {{_e:?}}");
            }
        });
    }
}

async fn handle_cli_connection(stream: UnixStream, app_state: AppState) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Ok(());
    }
    let line = line.trim();
    let mut parts = line.split_whitespace();

    match parts.next() {
        Some("add") => {
            if let Some(id_str) = parts.next() {
                if let Ok(id) = id_str.parse::<u64>() {
                    let full_window_list = get_full_window_list().await?;
                    if !full_window_list.contains(&id) {
                        writer.write_all(b"Window not found in Niri\n").await?;
                        return Ok(());
                    }
                    let mut sticky = app_state.sticky_windows.lock().await;
                    if sticky.insert(id) {
                        writer.write_all(b"Added\n").await?;
                    } else {
                        writer.write_all(b"Already in sticky list\n").await?;
                    }
                } else {
                    writer.write_all(b"Invalid window id\n").await?;
                }
            } else {
                writer.write_all(b"Missing window id\n").await?;
            }
        }

        Some("remove") => {
            if let Some(id_str) = parts.next() {
                if let Ok(id) = id_str.parse::<u64>() {
                    let full_window_list = get_full_window_list().await?;
                    if !full_window_list.contains(&id) {
                        writer.write_all(b"Window not found in Niri\n").await?;
                        return Ok(());
                    }
                    let mut sticky = app_state.sticky_windows.lock().await;
                    if sticky.remove(&id) {
                        writer.write_all(b"Removed\n").await?;
                    } else {
                        writer.write_all(b"Not in sticky list\n").await?;
                    }
                } else {
                    writer.write_all(b"Invalid window id\n").await?;
                }
            } else {
                writer.write_all(b"Missing window id\n").await?;
            }
        }

        Some("list") => {
            let snapshot: Vec<u64> = {
                let sticky = app_state.sticky_windows.lock().await;
                sticky.iter().copied().collect()
            };
            let full_window_list = get_full_window_list().await?;
            let valid_snapshot: Vec<u64> = snapshot
                .into_iter()
                .filter(|id| full_window_list.contains(id))
                .collect();
            let list_str = format!("{:?}\n", valid_snapshot);
            writer.write_all(list_str.as_bytes()).await?;
        }

        Some("toggle_active") => {
            let active_id = match get_active_window_id().await {
                Ok(id) => id,
                Err(_) => {
                    writer.write_all(b"Failed to get active window\n").await?;
                    return Ok(());
                }
            };
            let full_window_list = get_full_window_list().await?;
            if !full_window_list.contains(&active_id) {
                writer
                    .write_all(b"Active window not found in Niri\n")
                    .await?;
                return Ok(());
            }
            let mut sticky = app_state.sticky_windows.lock().await;
            if sticky.contains(&active_id) {
                sticky.remove(&active_id);
                writer
                    .write_all(b"Removed active window from sticky\n")
                    .await?;
            } else {
                sticky.insert(active_id);
                writer.write_all(b"Added active window to sticky\n").await?;
            }
        }

        Some("stage") => {
            let arg = parts.next();
            match arg {
                Some("--all") => {
                    let sticky_ids = app_state.sticky_windows.lock().await.clone();
                    if sticky_ids.is_empty() {
                        writer.write_all(b"No windows to stage\n").await?;
                        return Ok(());
                    }

                    let mut staged_count = 0;
                    let mut successfully_staged = HashSet::new();

                    for id in sticky_ids {
                        if move_to_named_workspace(id, "stage").await.is_ok() {
                            successfully_staged.insert(id);
                            staged_count += 1;
                        } else {
                            eprintln!("Failed to move window {{id}} to stage");
                        }
                    }

                    if staged_count > 0 {
                        let mut sticky = app_state.sticky_windows.lock().await;
                        let mut staged = app_state.staged_set.lock().await;
                        for id in &successfully_staged {
                            sticky.remove(id);
                            staged.insert(*id);
                        }
                    }

                    let response = format!("Staged {{staged_count}} windows\n");
                    writer.write_all(response.as_bytes()).await?;
                }
                Some("--list") => {
                    let staged = app_state.staged_set.lock().await;
                    let list_str = format!("{:?}\n", *staged);
                    writer.write_all(list_str.as_bytes()).await?;
                }
                Some("--active") => {
                    let id = match get_active_window_id().await {
                        Ok(id) => id,
                        Err(_) => {
                            writer.write_all(b"Failed to get active window\n").await?;
                            return Ok(());
                        }
                    };

                    let is_sticky = app_state.sticky_windows.lock().await.contains(&id);
                    if !is_sticky {
                        writer
                            .write_all(b"Active window is not sticky, cannot stage\n")
                            .await?;
                        return Ok(());
                    }

                    if let Err(_e) = move_to_named_workspace(id, "stage").await {
                        let response = format!("Failed to move window to stage: {{_e:?}}\n");
                        writer.write_all(response.as_bytes()).await?;
                    } else {
                        let mut sticky = app_state.sticky_windows.lock().await;
                        let mut staged = app_state.staged_set.lock().await;
                        sticky.remove(&id);
                        staged.insert(id);
                        writer.write_all(b"Staged active window\n").await?;
                    }
                }
                Some(id_str) => {
                    if let Ok(id) = id_str.parse::<u64>() {
                        let is_sticky = app_state.sticky_windows.lock().await.contains(&id);
                        if !is_sticky {
                            writer
                                .write_all(b"Window is not sticky, cannot stage\n")
                                .await?;
                            return Ok(());
                        }

                        if let Err(_e) = move_to_named_workspace(id, "stage").await {
                            let response = format!("Failed to move window to stage: {{_e:?}}\n");
                            writer.write_all(response.as_bytes()).await?;
                        } else {
                            let mut sticky = app_state.sticky_windows.lock().await;
                            let mut staged = app_state.staged_set.lock().await;
                            sticky.remove(&id);
                            staged.insert(id);
                            writer.write_all(b"Staged window\n").await?;
                        }
                    } else {
                        writer.write_all(b"Invalid window id\n").await?;
                    }
                }
                None => {
                    writer.write_all(b"Missing argument for stage\n").await?;
                }
            }
        }

        Some("unstage") => {
            let current_ws_id = match get_active_workspace_id().await {
                Ok(id) => id,
                Err(_) => {
                    writer.write_all(b"Failed to get active workspace ID\n").await?;
                    return Ok(());
                }
            };

            let arg = parts.next();
            match arg {
                Some("--all") => {
                    let ids_to_unstage: Vec<u64> = {
                        let staged = app_state.staged_set.lock().await;
                        if staged.is_empty() {
                            writer.write_all(b"No windows to unstage\n").await?;
                            return Ok(());
                        }
                        staged.iter().copied().collect()
                    };

                    let mut successfully_unstaged = HashSet::new();
                    for id in &ids_to_unstage {
                        if move_to_workspace(*id, current_ws_id).await.is_ok() {
                            successfully_unstaged.insert(*id);
                        } else {
                            eprintln!("Failed to move window {{id}} to workspace {{current_ws_id}}");
                        }
                    }

                    if !successfully_unstaged.is_empty() {
                        let mut staged = app_state.staged_set.lock().await;
                        let mut sticky = app_state.sticky_windows.lock().await;
                        for id in &successfully_unstaged {
                            staged.remove(id);
                            sticky.insert(*id);
                        }
                    }

                    let response = format!("Unstaged {{successfully_unstaged.len()}} windows\n");
                    writer.write_all(response.as_bytes()).await?;
                }
                Some("--active") => {
                    let id = match get_active_window_id().await {
                        Ok(id) => id,
                        Err(_) => {
                            writer.write_all(b"Failed to get active window\n").await?;
                            return Ok(());
                        }
                    };
                    let mut staged = app_state.staged_set.lock().await;
                    if !staged.contains(&id) {
                        writer.write_all(b"Active window is not staged\n").await?;
                        return Ok(());
                    }

                    if move_to_workspace(id, current_ws_id).await.is_ok() {
                        staged.remove(&id);
                        let mut sticky = app_state.sticky_windows.lock().await;
                        sticky.insert(id);
                        writer.write_all(b"Unstaged active window\n").await?;
                    } else {
                        let response = format!("Failed to move window {{id}} to workspace {{current_ws_id}}\n");
                        writer.write_all(response.as_bytes()).await?;
                    }
                }
                Some(id_str) => {
                    if let Ok(id) = id_str.parse::<u64>() {
                        let mut staged = app_state.staged_set.lock().await;
                        if !staged.contains(&id) {
                            writer.write_all(b"Window is not staged\n").await?;
                            return Ok(());
                        }

                        if move_to_workspace(id, current_ws_id).await.is_ok() {
                            staged.remove(&id);
                            let mut sticky = app_state.sticky_windows.lock().await;
                            sticky.insert(id);
                            writer.write_all(b"Unstaged window\n").await?;
                        } else {
                            let response = format!("Failed to move window {{id}} to workspace {{current_ws_id}}\n");
                            writer.write_all(response.as_bytes()).await?;
                        }
                    } else {
                        writer.write_all(b"Invalid window id\n").await?;
                    }
                }
                None => {
                    writer.write_all(b"Missing argument for unstage\n").await?;
                }
            }
        }

        _ => {
            writer.write_all(b"Unknown command\n").await?;
        }
    }

    Ok(())
}

async fn get_active_workspace_id() -> Result<u64> {
    let output = tokio::process::Command::new("niri")
        .args(["msg", "-j", "workspaces"])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!("Failed to get workspaces");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)?;

    if let Some(workspaces) = json.as_array() {
        for workspace in workspaces {
            if workspace.get("is_active").and_then(|v| v.as_bool()) == Some(true) {
                if let Some(id) = workspace.get("id").and_then(|v| v.as_u64()) {
                    return Ok(id);
                }
            }
        }
    }

    anyhow::bail!("Active workspace not found");
}

async fn get_active_window_id() -> Result<u64> {
    let output = tokio::process::Command::new("niri")
        .args(["msg", "--json", "focused-window"])
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!("Failed to get focused window");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)?;
    if let Some(id) = json.get("id").and_then(|v| v.as_u64()) {
        Ok(id)
    } else {
        anyhow::bail!("Focused window id not found");
    }
}

async fn run_watcher(app_state: AppState) -> Result<()> {
    let socket_path = std::env::var("NIRI_SOCKET").expect("NIRI_SOCKET env var not set");
    let stream = UnixStream::connect(&socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    writer.write_all(b"\"EventStream\"\n").await?;
    writer.flush().await?;

    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            if let Some(ws) = v.get("WorkspaceActivated") {
                if let Some(ws_id) = ws.get("id").and_then(|id| id.as_u64()) {
                    println!("Workspace switched to: {{ws_id}}");

                    let sticky_snapshot = {
                        let mut sticky = app_state.sticky_windows.lock().await;
                        let full_window_list = get_full_window_list().await.unwrap_or_default();
                        sticky.retain(|win_id| full_window_list.contains(win_id));
                        println!("Updated sticky windows: {:?}", *sticky);
                        sticky.clone()
                    };

                    for win_id in sticky_snapshot.iter() {
                        if let Err(_e) = move_to_workspace(*win_id, ws_id).await {
                            eprintln!("Failed to move window {{win_id}}: {{_e:?}}");
                        }
                    }
                }
            }
        }
        line.clear();
    }

    Ok(())
}

async fn get_full_window_list() -> Result<HashSet<u64>> {
    let output = Command::new("niri")
        .args(["msg", "--json", "windows"])
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!("Failed to get windows list");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout)?;
    let mut window_ids = HashSet::new();
    if let Some(arr) = json.as_array() {
        for item in arr {
            if let Some(id) = item.get("id").and_then(|v| v.as_u64()) {
                window_ids.insert(id);
            }
        }
    }
    Ok(window_ids)
}

async fn move_to_workspace(win_id: u64, ws_id: u64) -> Result<()> {
    let socket_path = std::env::var("NIRI_SOCKET")?;

    let stream = UnixStream::connect(&socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let cmd = json!({
        "Action": {
            "MoveWindowToWorkspace": {
                "window_id": win_id,
                "focus": false,
                "reference": { "Id": ws_id }
            }
        }
    });
    let cmd_str = serde_json::to_string(&cmd)? + "\n";

    writer.write_all(cmd_str.as_bytes()).await?;
    writer.flush().await?;

    let mut response = String::new();
    reader.read_line(&mut response).await?;
    println!("move_to_workspace response: {}", response.trim());
    Ok(())
}

async fn move_to_named_workspace(win_id: u64, workspace_name: &str) -> Result<()> {
    let socket_path = std::env::var("NIRI_SOCKET")?;
    let stream = UnixStream::connect(&socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let cmd = json!({
        "Action": {
            "MoveWindowToWorkspace": {
                "window_id": win_id,
                "focus": false,
                "reference": { "Name": workspace_name }
            }
        }
    });
    let cmd_str = serde_json::to_string(&cmd)? + "\n";
    writer.write_all(cmd_str.as_bytes()).await?;
    writer.flush().await?;
    let mut response = String::new();
    reader.read_line(&mut response).await?;
    println!("move_to_named_workspace response: {}", response.trim());
    Ok(())
}