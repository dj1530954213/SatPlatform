// SatControlCenter/src-tauri/src/commands/app_lifecycle_cmds.rs
// 占位符：应用生命周期相关命令
use tauri::{AppHandle, Emitter}; // 移除了 Manager
use log::info;

#[tauri::command]
pub async fn on_window_ready(app_handle: AppHandle) -> Result<(), String> {
    info!("[中心端CMD] on_window_ready command called.");
    // 示例：当窗口准备好时，可以发送一个事件到前端
    if let Err(e) = app_handle.emit("app_window_ready_event", Some("Window is ready from Rust!")) {
        info!("[中心端CMD] Failed to emit app_window_ready_event: {:?}", e);
    }
    Ok(())
} 