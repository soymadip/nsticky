use anyhow::Result;
use std::collections::HashSet;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct BusinessLogic {
    sticky_windows: std::sync::Arc<Mutex<HashSet<u64>>>,
    staged_set: std::sync::Arc<Mutex<HashSet<u64>>>,
}

impl BusinessLogic {
    pub fn new(
        sticky_windows: std::sync::Arc<Mutex<HashSet<u64>>>,
        staged_set: std::sync::Arc<Mutex<HashSet<u64>>>,
    ) -> Self {
        Self {
            sticky_windows,
            staged_set,
        }
    }

    pub async fn add_sticky_window(&self, window_id: u64) -> Result<bool> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&window_id) {
            return Err(anyhow::anyhow!("Window not found in Niri"));
        }

        let mut sticky = self.sticky_windows.lock().await;
        Ok(sticky.insert(window_id))
    }

    pub async fn remove_sticky_window(&self, window_id: u64) -> Result<bool> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&window_id) {
            return Err(anyhow::anyhow!("Window not found in Niri"));
        }

        let mut sticky = self.sticky_windows.lock().await;
        Ok(sticky.remove(&window_id))
    }

    pub async fn list_sticky_windows(&self) -> Result<Vec<u64>> {
        let snapshot: Vec<u64> = {
            let sticky = self.sticky_windows.lock().await;
            sticky.iter().copied().collect()
        };
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        let valid_snapshot: Vec<u64> = snapshot
            .into_iter()
            .filter(|id| full_window_list.contains(id))
            .collect();
        Ok(valid_snapshot)
    }

    pub async fn toggle_active_window(&self) -> Result<bool> {
        let active_id = crate::system_integration::get_active_window_id().await?;
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&active_id) {
            return Err(anyhow::anyhow!("Active window not found in Niri"));
        }

        let mut sticky = self.sticky_windows.lock().await;
        if sticky.contains(&active_id) {
            sticky.remove(&active_id);
            Ok(false) // Removed from sticky
        } else {
            sticky.insert(active_id);
            Ok(true) // Added to sticky
        }
    }


    pub async fn stage_window(&self, window_id: u64) -> Result<()> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&window_id) {
            return Err(anyhow::anyhow!("Window not found in Niri"));
        }

        let was_sticky = {
            let mut sticky = self.sticky_windows.lock().await;
            let was_sticky = sticky.contains(&window_id);
            if was_sticky {
                sticky.remove(&window_id);
            }
            was_sticky
        };

        if !was_sticky {
            return Err(anyhow::anyhow!("Window is not sticky, cannot stage"));
        }

        if let Err(e) = crate::system_integration::move_to_named_workspace(window_id, "stage").await {
            let mut sticky = self.sticky_windows.lock().await;
            sticky.insert(window_id);
            return Err(e);
        }

        let mut staged = self.staged_set.lock().await;
        staged.insert(window_id);

        Ok(())
    }

    pub async fn stage_active_window(&self) -> Result<()> {
        let id = crate::system_integration::get_active_window_id().await?;

        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&id) {
            return Err(anyhow::anyhow!("Active window not found in Niri"));
        }

        let was_sticky = {
            let mut sticky = self.sticky_windows.lock().await;
            let was_sticky = sticky.contains(&id);
            if was_sticky {
                sticky.remove(&id);
            }
            was_sticky
        };

        if !was_sticky {
            return Err(anyhow::anyhow!("Window is not sticky, cannot stage"));
        }

        if let Err(e) = crate::system_integration::move_to_named_workspace(id, "stage").await {
            let mut sticky = self.sticky_windows.lock().await;
            sticky.insert(id);
            return Err(e);
        }

        let mut staged = self.staged_set.lock().await;
        staged.insert(id);

        Ok(())
    }

    pub async fn is_window_staged(&self, window_id: u64) -> bool {
        let staged = self.staged_set.lock().await;
        staged.contains(&window_id)
    }


    pub async fn stage_all_windows(&self) -> Result<usize> {
        let sticky_ids = self.sticky_windows.lock().await.clone();
        if sticky_ids.is_empty() {
            return Ok(0);
        }

        let mut successfully_staged = Vec::new();

        let full_window_list = crate::system_integration::get_full_window_list().await?;
        let valid_sticky_ids: Vec<u64> = sticky_ids
            .into_iter()
            .filter(|id| full_window_list.contains(id))
            .collect();

        for id in valid_sticky_ids {
            if crate::system_integration::move_to_named_workspace(id, "stage").await.is_ok() {
                successfully_staged.push(id);
            } else {
                eprintln!("Failed to move window {} to stage", id);
            }
        }

        let mut sticky = self.sticky_windows.lock().await;
        let mut staged = self.staged_set.lock().await;
        for id in &successfully_staged {
            sticky.remove(id);
            staged.insert(*id);
        }

        Ok(successfully_staged.len())
    }

    pub async fn list_staged_windows(&self) -> Result<Vec<u64>> {
        let staged = self.staged_set.lock().await;
        Ok(staged.iter().copied().collect())
    }

    pub async fn unstage_window(&self, window_id: u64, workspace_id: u64) -> Result<()> {
        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&window_id) {
            return Err(anyhow::anyhow!("Window not found in Niri"));
        }

        let was_staged = {
            let mut staged = self.staged_set.lock().await;
            let was_staged = staged.contains(&window_id);
            if was_staged {
                staged.remove(&window_id);
            }
            was_staged
        };

        if !was_staged {
            return Err(anyhow::anyhow!("Window is not staged"));
        }

        if let Err(e) = crate::system_integration::move_to_workspace(window_id, workspace_id).await {
            let mut staged = self.staged_set.lock().await;
            staged.insert(window_id);
            return Err(e);
        }

        let mut sticky = self.sticky_windows.lock().await;
        sticky.insert(window_id);

        Ok(())
    }

    pub async fn unstage_active_window(&self, workspace_id: u64) -> Result<()> {
        let id = crate::system_integration::get_active_window_id().await?;

        let full_window_list = crate::system_integration::get_full_window_list().await?;
        if !full_window_list.contains(&id) {
            return Err(anyhow::anyhow!("Active window not found in Niri"));
        }

        let was_staged = {
            let mut staged = self.staged_set.lock().await;
            let was_staged = staged.contains(&id);
            if was_staged {
                staged.remove(&id);
            }
            was_staged
        };

        if !was_staged {
            return Err(anyhow::anyhow!("Active window is not staged"));
        }

        if let Err(e) = crate::system_integration::move_to_workspace(id, workspace_id).await {
            let mut staged = self.staged_set.lock().await;
            staged.insert(id);
            return Err(e);
        }

        let mut sticky = self.sticky_windows.lock().await;
        sticky.insert(id);

        Ok(())
    }


    pub async fn unstage_all_windows(&self, workspace_id: u64) -> Result<usize> {
        let ids_to_unstage: Vec<u64> = {
            let staged = self.staged_set.lock().await;
            if staged.is_empty() {
                return Ok(0);
            }
            staged.iter().copied().collect()
        };

        let full_window_list = crate::system_integration::get_full_window_list().await?;
        let valid_ids_to_unstage: Vec<u64> = ids_to_unstage
            .into_iter()
            .filter(|id| full_window_list.contains(id))
            .collect();

        let mut successfully_unstaged = Vec::new();
        for id in &valid_ids_to_unstage {
            if crate::system_integration::move_to_workspace(*id, workspace_id).await.is_ok() {
                successfully_unstaged.push(*id);
            } else {
                eprintln!("Failed to move window {} to workspace {}", id, workspace_id);
            }
        }

        let mut staged = self.staged_set.lock().await;
        let mut sticky = self.sticky_windows.lock().await;
        for id in &successfully_unstaged {
            staged.remove(id);
            sticky.insert(*id);
        }

        Ok(successfully_unstaged.len())
    }

    pub async fn handle_workspace_activation(&self, ws_id: u64) -> Result<()> {
        // 更新粘性窗口列表，移除不再存在的窗口
        let sticky_snapshot = {
            let mut sticky = self.sticky_windows.lock().await;
            let full_window_list = crate::system_integration::get_full_window_list().await.unwrap_or_default();
            sticky.retain(|win_id| full_window_list.contains(win_id));
            println!("Updated sticky windows: {:?}", *sticky);
            sticky.clone()
        };

        // 将粘性窗口移动到新工作区
        for win_id in sticky_snapshot.iter() {
            if let Err(_e) = crate::system_integration::move_to_workspace(*win_id, ws_id).await {
                eprintln!("Failed to move window {}: {:?}", win_id, _e);
            }
        }

        Ok(())
    }
}