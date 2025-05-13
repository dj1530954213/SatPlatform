// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// 模块声明区域
mod config;     // 应用配置模块 (P2.1.1 - 现场端)
mod event;      // Tauri 事件定义模块 (P2.1.1 - 现场端)
mod ws_client;  // WebSocket 客户端服务模块 (P2.1.1 - 现场端)
mod commands;   // Tauri 命令定义模块 (P2.1.1 - 现场端)

use std::sync::Arc;
use crate::ws_client::service::WebSocketClientService;
use log::{info, LevelFilter};
use tauri::Manager; // 引入 tauri::Manager trait 以使用 app.manage()
use env_logger;

/// `SatOnSiteMobile` (现场端) 应用程序的主入口函数。
fn main() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .filter_module("SatOnSiteMobile", LevelFilter::Debug)
        .filter_module("common_models", LevelFilter::Debug)
        .try_init()
        .expect("初始化 env_logger 失败 (SatOnSiteMobile)");

    info!("SatOnSiteMobile 应用启动中...");

    tauri::Builder::default()
        .setup(|app| {
            info!("Tauri setup 钩子执行 (SatOnSiteMobile)...");
            let app_handle = app.handle();

            // 创建新的 WebSocketClientService 实例
            let ws_service_instance = Arc::new(WebSocketClientService::new(app_handle.clone()));
            
            // 将新的服务实例放入 Tauri 状态管理
            app.manage(ws_service_instance);
            info!("WebSocketClientService 已放入 Tauri 状态管理 (SatOnSiteMobile)。");

            info!("Tauri setup 钩子执行完毕 (SatOnSiteMobile)。");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::general_cmds::connect_to_cloud,
            commands::general_cmds::disconnect_from_cloud,
            commands::general_cmds::check_ws_connection_status,
            commands::general_cmds::send_ws_echo,
            commands::general_cmds::register_client_with_task,
            // 其他 SatOnSiteMobile 可能需要的命令
        ])
        .build(tauri::generate_context!())
        .expect("构建 Tauri 应用失败 (SatOnSiteMobile)。")
        .run(|_app_handle, event| match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                info!("Tauri RunEvent: ExitRequested (SatOnSiteMobile).");
                // api.prevent_exit(); // 移除阻止退出，改为直接退出
                // 如果有需要异步完成的清理工作，可以在这里执行，
                // 但最终必须手动退出。
                // 这里我们直接退出。
                std::process::exit(0);
            }
            tauri::RunEvent::Exit => {
                info!("Tauri RunEvent: 应用即将退出 (SatOnSiteMobile)。");
            }
            _ => {}
        });
    info!("Tauri 应用 run 方法已调用 (SatOnSiteMobile)。");
}
