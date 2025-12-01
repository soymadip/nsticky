use anyhow::Result;
use serde_json::{Value, json};
use std::collections::HashSet;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    process::Command,
};

// 与Niri进行交互的函数
pub async fn get_active_workspace_id() -> Result<u64> {
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
            if workspace.get("is_active").and_then(|v| v.as_bool()) == Some(true)
                && let Some(id) = workspace.get("id").and_then(|v| v.as_u64())
            {
                return Ok(id);
            }
        }
    }

    anyhow::bail!("Active workspace not found");
}

pub async fn get_active_window_id() -> Result<u64> {
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

pub async fn get_full_window_list() -> Result<HashSet<u64>> {
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

pub async fn move_to_workspace(win_id: u64, ws_id: u64) -> Result<()> {
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

pub async fn move_to_named_workspace(win_id: u64, workspace_name: &str) -> Result<()> {
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
