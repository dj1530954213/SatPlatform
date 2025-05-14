// SatOnSiteMobile/src-tauri/src/commands/dev_tools_cmds.rs
// Placeholder for Developer Tools Commands 

#[tauri::command]
pub fn open_dev_tools(_window: tauri::Window) -> Result<(), String> { // 参数 window 暂时标记为未使用
    // 在 release 版本中，开发者工具通常不可用，这里做个简单判断
    // #[cfg(debug_assertions)]
    // {
    //     if window.is_devtools_open() {
    //         window.close_devtools();
    //     } else {
    //         window.open_devtools();
    //     }
    // }
    log::warn!("[DevToolsCMD::open_dev_tools] Devtools functionality is currently disabled due to incompatibility with the tauri version/features used.");
    Ok(())
} 