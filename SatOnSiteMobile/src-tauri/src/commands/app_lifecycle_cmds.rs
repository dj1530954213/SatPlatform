// SatOnSiteMobile/src-tauri/src/commands/app_lifecycle_cmds.rs
use tauri::Emitter; // 引入 Emitter trait

#[tauri::command]
pub fn on_window_ready(app_handle: tauri::AppHandle) -> Result<(), String> {
    log::info!("[AppLifecycleCMD::on_window_ready] Main window is ready.");
    // 可以在这里执行窗口就绪后的初始化逻辑，例如发送一个事件通知其他部分
    // 使用 emit 根据编译器建议，如果需要全局事件，可能需要调整事件监听逻辑或使用其他方式
    app_handle.emit("app_window_ready", ()).map_err(|e| e.to_string())?;
    Ok(())
} 