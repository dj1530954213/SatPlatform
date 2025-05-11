use tauri::Manager; // 重新添加，以备后用或确认是否确实无关
use log::{error, info, LevelFilter}; // 引入 LevelFilter
use sat_cloud_service::ws_server::service::WsService; // 引入 WsService
use sat_cloud_service::ws_server::connection_manager::ConnectionManager; // 引入 ConnectionManager
use sat_cloud_service::ws_server::task_state_manager::TaskStateManager; // P3.1.2: 引入 TaskStateManager
use sat_cloud_service::ws_server::heartbeat_monitor::HeartbeatMonitor; // P3.2.1: 引入 HeartbeatMonitor
use std::sync::Arc;
use std::time::Duration; // P3.2.1: 引入 Duration

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// 在 Windows 发行版中阻止额外的控制台窗口，请勿删除!!
#[cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
// 进一步了解 Tauri 命令: https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

mod config;
mod ws_server;

fn main() {
    // 初始化日志记录器 (env_logger)
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info) // 默认设置为 Info 级别
        .format_timestamp_millis()       // 添加毫秒级时间戳
        .init();
    info!("日志系统已初始化。");

    // 启动 Tauri 应用
    tauri::Builder::default()
        .setup(|app| {
            info!("Tauri App Setup: 初始化开始");

            // 使用 app.handle() 传递给 init_config
            sat_cloud_service::config::init_config(&app.handle()); 
            let app_config = sat_cloud_service::config::get_config();
            info!("[MainSetup] 应用配置已加载: {:?}", app_config);

            // 创建 TaskStateManager (P3.1.2)
            let task_state_manager = Arc::new(TaskStateManager::new());
            info!("[MainSetup] TaskStateManager (骨架) 已创建。");

            // 创建 ConnectionManager 并注入 TaskStateManager (P1.2.1 & P3.1.2)
            let connection_manager = Arc::new(ConnectionManager::new(task_state_manager.clone()));
            info!("[MainSetup] ConnectionManager 已创建并注入 TaskStateManager。");

            // 将 ConnectionManager 放入 Tauri 的 State 中，以便 Tauri 命令可以访问 (P1.2.1)
            app.manage(connection_manager.clone()); 
            info!("[MainSetup] ConnectionManager 已放入 Tauri State。");

            let app_handle_for_ws = app.handle().clone(); // Clone AppHandle for ws_service
            let ws_service_instance = WsService::new(app_config.websocket.clone(), connection_manager.clone());
            
            // 使用 tauri::async_runtime::spawn
            tauri::async_runtime::spawn(async move {
                info!("正在创建并启动 WebSocket 服务任务...");
                if let Err(e) = ws_service_instance.start().await { 
                    error!("启动 WebSocket 服务失败: {}", e);
                }
            });
            info!("[MainSetup] WebSocket 服务启动任务已异步派生。");

            let heartbeat_check_interval = Duration::from_secs(app_config.websocket.heartbeat_check_interval_seconds);
            let client_timeout = Duration::from_secs(app_config.websocket.client_timeout_seconds);
            
            let app_handle_for_heartbeat = app.handle().clone(); // Clone AppHandle for heartbeat_monitor (if needed, though not directly used by it currently)
            let heartbeat_monitor = HeartbeatMonitor::new(
                connection_manager.clone(),
                client_timeout,
                heartbeat_check_interval,
            );
            // 使用 tauri::async_runtime::spawn
            tauri::async_runtime::spawn(async move {
                info!("[MainSetup] 启动 HeartbeatMonitor 任务...");
                heartbeat_monitor.run().await;
            });
            info!("[MainSetup] HeartbeatMonitor 启动任务已异步派生。");

            info!("Tauri App Setup: 初始化完成");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![]) // 确保 invoke_handler 存在，即使为空
        .run(tauri::generate_context!()) 
        .expect("运行 Tauri 应用时发生错误");
}
