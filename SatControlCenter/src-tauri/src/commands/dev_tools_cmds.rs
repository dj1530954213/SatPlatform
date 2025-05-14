// SatControlCenter/src-tauri/src/commands/dev_tools_cmds.rs
// 占位符：开发者工具相关命令
// use tauri::Manager; // 移除未使用的导入
use log::{info, warn};

#[tauri::command]
pub async fn open_dev_tools(window: tauri::Window) -> Result<(), String> {
    info!("[中心端CMD] open_dev_tools command called for window: {}.", window.label());
    // 注意：Tauri v2 中 window.open_devtools() 等方法可能已变更或需要特定特性。
    // 在 SatOnSiteMobile 中，此功能由于特性问题被临时禁用了。
    // 中心端如果需要此功能，需确保 Tauri 依赖中启用了正确的特性。
    // 为保持同步，暂时也只打印日志。
    /* 
    if window.is_devtools_open() {
        info!("[中心端CMD] DevTools is already open for window: {}", window.label());
        // window.close_devtools(); // 示例：如果需要关闭
    } else {
        info!("[中心端CMD] Opening DevTools for window: {}", window.label());
        // window.open_devtools(); // 示例：如果需要打开
        // window.open_devtools_unrestricted(); // 另一个可能的API，取决于Tauri版本和配置
    }
    */
    warn!("[中心端CMD] DevTools functionality is currently disabled pending feature review for Tauri v2.");
    Ok(())
} 