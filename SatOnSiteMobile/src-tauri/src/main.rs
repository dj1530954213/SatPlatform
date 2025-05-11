// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// 模块声明区域
mod config;     // 应用配置模块 (P2.1.1 - 现场端)
mod event;      // Tauri 事件定义模块 (P2.1.1 - 现场端)
mod ws_client;  // WebSocket 客户端服务模块 (P2.1.1 - 现场端)
mod commands;   // Tauri 命令定义模块 (P2.1.1 - 现场端)

use std::sync::Arc;
use ws_client::WebSocketClientService; // 引入 WebSocket 客户端服务
use log::{info, error}; // 引入日志宏
use tauri::Manager; // 引入 tauri::Manager trait 以使用 app.manage()

/// `SatOnSiteMobile` (现场端) 应用程序的主入口函数。
fn main() {
    // 初始化日志记录器
    // 如果 RUST_LOG 环境变量未设置，则默认为 "debug" 级别。
    // 这允许在运行时通过设置 RUST_LOG 环境变量来调整日志输出的详细程度，
    // 例如 RUST_LOG=info cargo tauri dev 只会显示 info 及更高级别的日志。
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "debug"); 
    }
    env_logger::init(); // 初始化 env_logger
    info!("[SatOnSiteMobile] 现场端应用程序启动中...");

    // 构建并运行 Tauri 应用程序实例
    tauri::Builder::default()
        .setup(|app| {
            // 此闭包在 Tauri 应用构建完成但在任何窗口创建之前执行。
            // 非常适合用于初始化后端服务和设置共享状态。
            info!("[SatOnSiteMobile] Tauri 应用 Setup 阶段: 正在初始化 WebSocketClientService...");
            
            // 获取应用句柄的克隆，以便传递给服务
            let app_handle_clone = app.handle().clone();
            
            // 创建 WebSocketClientService 实例，并使用 Arc 进行共享
            let ws_service = Arc::new(WebSocketClientService::new(app_handle_clone));
            
            // 将 WebSocketClientService 实例注入到 Tauri 的状态管理器中，
            // 这样它就可以在 Tauri 命令中通过 `State<Arc<WebSocketClientService>>` 被访问。
            app.manage(ws_service.clone()); 
            // 保留 .clone() 是一个好习惯，以防 ws_service 的所有权后续仍需在此作用域内使用，
            // 尽管在这里它之后没有被直接使用。
            
            info!("[SatOnSiteMobile] WebSocketClientService 已成功创建并注入到 Tauri State 管理。");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // 注册所有需要从前端调用的 Tauri 命令
            // 这些命令定义在 `commands` 模块的相应子模块中 (如此处的 `general_cmds`)
            commands::general_cmds::connect_to_cloud,           // 连接到云端 WebSocket
            commands::general_cmds::disconnect_from_cloud,      // 从云端 WebSocket 断开
            commands::general_cmds::check_ws_connection_status, // 检查 WebSocket 连接状态
            commands::general_cmds::send_ws_echo,               // 发送 Echo 测试消息
            commands::general_cmds::register_client_with_task,  // 注册客户端并关联任务 (P4.1.1)
            // 注意: 如果在 `commands::general_cmds.rs` 中未定义某个命令 (例如，由于从 SatControlCenter 复制时遗漏)，
            // 则此处会导致编译错误。这有助于在编译阶段发现命令注册问题。
        ])
        .run(tauri::generate_context!())
        .expect("错误：SatOnSiteMobile 现场端应用程序运行失败，请检查日志获取更多信息。");
    
    // 这条日志通常在应用正常关闭（例如，用户关闭最后一个窗口）时不会执行，
    // 因为 .run() 方法是阻塞的，直到应用完全退出。
    // info!("[SatOnSiteMobile] 现场端应用程序已关闭。"); 
}
