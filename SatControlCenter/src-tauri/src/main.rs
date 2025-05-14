// 除非在调试模式下，否则在 Windows 发布版本中阻止打开额外的控制台窗口。
// 请勿移除此行!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// --- 模块声明 --- 
mod config;     // 应用配置模块 (P2.1.1 - 中心端)
mod event;      // Tauri 事件定义模块 (P2.1.1 - 中心端)
mod ws_client;  // WebSocket 客户端服务模块 (P2.1.1 - 中心端)
mod commands;   // Tauri 命令定义模块 (P2.1.1 - 中心端)
// mod error; // 如果有全局错误处理模块，请取消注释
// mod state; // 如果有全局状态管理模块（除了Tauri管理的），请取消注释

// --- 依赖引入 --- 
use std::sync::Arc;
use crate::ws_client::service::WebSocketClientService; // WebSocket 客户端服务
use log::{info, LevelFilter}; // 日志记录宏和级别过滤器
use tauri::Manager; // 引入 tauri::Manager trait 以便使用 app.manage() 方法管理状态
use env_logger; // 基于环境变量的日志记录器

/// `SatControlCenter` (中心端) 的主入口点。
///
/// ## 主要职责：
/// 1. 初始化日志系统 (`env_logger`)。
/// 2. 构建并配置 Tauri 应用实例：
///    - 在 `setup` 钩子中初始化并管理 `WebSocketClientService` 状态。
///    - 注册所有由前端调用的 Tauri 命令 (`invoke_handler`)。
/// 3. 运行 Tauri 应用，并处理其生命周期事件（例如退出请求）。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 env_logger 日志记录器
    // 默认级别为 Info，但 SatControlCenter 和 common_models 模块的日志级别设置为 Debug
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info) // 全局默认日志级别
        .filter_module("SatControlCenter", LevelFilter::Debug) // 本应用的调试日志
        .filter_module("common_models", LevelFilter::Debug)   // 共享模型的调试日志
        .try_init()
        .expect("初始化 env_logger 日志服务失败 (SatControlCenter 应用)");

    info!("中心端应用 (SatControlCenter) 正在启动...");

    // 构建 Tauri 应用
    let app = tauri::Builder::default()
        .setup(|app| {
            info!("Tauri 应用的 setup 钩子函数开始执行 (SatControlCenter)...");
            let app_handle = app.handle(); // 获取应用句柄，用于后续操作，如事件发射和服务创建

            // --- WebSocket 客户端服务初始化与管理 ---
            // 创建 WebSocketClientService 的实例，使用 Arc 进行原子引用计数，以便安全共享
            let ws_service_instance = Arc::new(WebSocketClientService::new(app_handle.clone()));
            
            // 将 WebSocketClientService 实例注册到 Tauri 的状态管理器中，
            // 这样就可以在 Tauri 命令处理函数中通过 AppHandle::state() 来访问它。
            app.manage(ws_service_instance);
            info!("WebSocketClientService 实例已成功注册到 Tauri 状态管理器 (SatControlCenter)。");

            info!("Tauri 应用的 setup 钩子函数执行完毕 (SatControlCenter)。");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Removed greet, get_system_info, frontend_ready as they are not defined in general_cmds.rs
            // Removed execute_task_command, get_task_state as task_cmds.rs is a placeholder
            // Removed get_cloud_connection_state, get_cloud_assigned_client_id, 
            // get_latest_cloud_error_message, clear_latest_cloud_error_message as they are not defined in general_cmds.rs
            
            // Keeping commands that are confirmed to exist in general_cmds.rs
            commands::general_cmds::connect_to_cloud,
            commands::general_cmds::disconnect_from_cloud,
            commands::general_cmds::check_ws_connection_status,
            commands::general_cmds::send_ws_echo,
            commands::general_cmds::register_client_with_task
        ])
        .on_window_event(|window, event_type| match event_type {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // 阻止窗口默认关闭行为
                // api.prevent_close();
                info!("CloseRequested event received for window: {}", window.label());
                // let window_clone = window.clone(); // Need to clone window for use in confirm dialog closure
                // // Show a confirmation dialog
                // tauri::api::dialog::confirm(
                //     Some(&window_clone),
                //     "Confirm Exit",
                //     "Are you sure you want to close SatControlCenter?",
                //     move |response| {
                //         if response {
                //             info!("User confirmed exit. Closing window.");
                //             // If user confirms, then close the window
                //             // window_clone.close().expect("Failed to close window");
                //             std::process::exit(0); // Exit the entire application
                //         } else {
                //             info!("User cancelled exit.");
                //         }
                //     },
                // );
            }
            _ => {}
        })
        .build(tauri::generate_context!()) // Ensure this is correctly placed
        .expect("error while building tauri application");

    // Setup application event listeners
    // setup_event_listeners(app.handle()); // Temporarily commented out
    let app_handle = app.handle().clone();
    // tauri::async_runtime::spawn(async move {
    //     setup_event_listeners(&app_handle).await; // Temporarily commented out
    // });

    app.run(|_app_handle, event| match event {
        tauri::RunEvent::ExitRequested { .. } => { // Removed api as it's unused
            info!("ExitRequested run event received. Application will terminate.");
            // Perform any cleanup before exiting if necessary
        }
        tauri::RunEvent::Ready => {
            info!("Application is ready.");
        }
        _ => {}
    });

    Ok(())
}
