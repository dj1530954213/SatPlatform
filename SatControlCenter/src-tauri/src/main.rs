// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;
// 导入模块
mod config;
mod event;
mod ws_client;
mod commands;

use std::sync::Arc;
use ws_client::WebSocketClientService;

fn main() {
    // 初始化日志记录器 (env_logger 或 tracing_subscriber)
    // env_logger::init(); // 在实际应用中取消注释并配置
    // 为了简单起见，如果 tauri-plugin-log 已在 Cargo.toml 中且希望使用它，则不需要手动 env_logger::init()
    // 但如果想用标准 log facade + env_logger，则需要手动初始化。
    // 根据 P2.1.1，明确要求 env_logger 或 tracing-subscriber。这里假设 env_logger。
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "debug"); // 设置默认日志级别为 debug
    }
    env_logger::init();
    log::info!("SatControlCenter 应用启动中...");

    tauri::Builder::default()
        .setup(|app| {
            log::info!("Tauri setup hook: 初始化 WebSocketClientService...");
            // 创建 WebSocketClientService 实例
            let ws_service = Arc::new(WebSocketClientService::new(app.handle().clone()));
            // 将服务实例放入 Tauri 的 State，以便在命令中访问
            app.manage(ws_service);
            log::info!("WebSocketClientService 已放入 Tauri State.");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::general_cmds::connect_to_cloud,
            commands::general_cmds::disconnect_from_cloud,
            commands::general_cmds::check_ws_connection_status,
            commands::general_cmds::send_ws_echo
            // 在这里添加其他 Tauri 命令
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用时发生错误");
}
