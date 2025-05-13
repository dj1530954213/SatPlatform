// 除非在调试模式下，否则在 Windows 发布版本中阻止打开额外的控制台窗口。
// 请勿移除此行!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// --- 模块声明 --- 
mod config;     // 应用配置模块 (P2.1.1 - 现场端)
mod event;      // Tauri 事件定义模块 (P2.1.1 - 现场端)
mod ws_client;  // WebSocket 客户端服务模块 (P2.1.1 - 现场端)
mod commands;   // Tauri 命令定义模块 (P2.1.1 - 现场端)
// mod error; // 如果有全局错误处理模块，请取消注释
// mod state; // 如果有全局状态管理模块（除了Tauri管理的），请取消注释

// --- 依赖引入 --- 
use std::sync::Arc;
use crate::ws_client::service::WebSocketClientService; // WebSocket 客户端服务
use log::{info, LevelFilter}; // 日志记录宏和级别过滤器
use tauri::Manager; // 引入 tauri::Manager trait 以便使用 app.manage() 方法管理状态
use env_logger; // 基于环境变量的日志记录器

/// `SatOnSiteMobile` (现场端移动应用) 的主入口点。
///
/// ## 主要职责：
/// 1. 初始化日志系统 (`env_logger`)。
/// 2. 构建并配置 Tauri 应用实例：
///    - 在 `setup` 钩子中初始化并管理 `WebSocketClientService` 状态。
///    - 注册所有由前端调用的 Tauri 命令 (`invoke_handler`)。
/// 3. 运行 Tauri 应用，并处理其生命周期事件（例如退出请求）。
fn main() {
    // 初始化 env_logger 日志记录器
    // 默认级别为 Info，但 SatOnSiteMobile 和 common_models 模块的日志级别设置为 Debug
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info) // 全局默认日志级别
        .filter_module("SatOnSiteMobile", LevelFilter::Debug) // 本应用的调试日志
        .filter_module("common_models", LevelFilter::Debug)   // 共享模型的调试日志
        .try_init()
        .expect("初始化 env_logger 日志服务失败 (SatOnSiteMobile 应用)");

    info!("现场端移动应用 (SatOnSiteMobile) 正在启动...");

    // 构建 Tauri 应用
    tauri::Builder::default()
        .setup(|app| {
            info!("Tauri 应用的 setup 钩子函数开始执行 (SatOnSiteMobile)...");
            let app_handle = app.handle(); // 获取应用句柄，用于后续操作，如事件发射和服务创建

            // --- WebSocket 客户端服务初始化与管理 ---
            // 创建 WebSocketClientService 的实例，使用 Arc 进行原子引用计数，以便安全共享
            let ws_service_instance = Arc::new(WebSocketClientService::new(app_handle.clone()));
            
            // 将 WebSocketClientService 实例注册到 Tauri 的状态管理器中，
            // 这样就可以在 Tauri 命令处理函数中通过 AppHandle::state() 来访问它。
            app.manage(ws_service_instance);
            info!("WebSocketClientService 实例已成功注册到 Tauri 状态管理器 (SatOnSiteMobile)。");

            info!("Tauri 应用的 setup 钩子函数执行完毕 (SatOnSiteMobile)。");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // --- 注册 Tauri 命令 --- 
            // 这些命令定义在 commands 模块中，允许前端通过 invoke API 调用
            commands::general_cmds::connect_to_cloud,         // 连接到云端 WebSocket 服务
            commands::general_cmds::disconnect_from_cloud,    // 从云端断开连接
            commands::general_cmds::check_ws_connection_status, // 检查当前 WebSocket 连接状态
            commands::general_cmds::send_ws_echo,             // 发送 Echo 消息到云端
            commands::general_cmds::register_client_with_task, // 注册客户端到任务组
            // TODO: 在此根据 SatOnSiteMobile 的具体业务需求，添加更多 Tauri 命令。
            // 例如：commands::pre_check_cmds::update_pre_check_item,
            //       commands::single_test_cmds::send_test_feedback,
        ])
        .build(tauri::generate_context!()) // 根据 tauri.conf.json 和 Cargo.toml 生成上下文
        .expect("构建 Tauri 应用核心失败 (SatOnSiteMobile)，请检查配置。")
        .run(|_app_handle, event| match event {
            // --- Tauri 应用运行时事件处理 --- 
            tauri::RunEvent::ExitRequested { api, .. } => {
                // 当用户尝试关闭应用窗口时触发（例如点击关闭按钮）
                info!("Tauri 运行时事件：用户请求退出应用 (SatOnSiteMobile)。");
                // api.prevent_exit(); // 如果需要阻止立即退出（例如执行清理操作），可以调用此方法
                
                // 当前实现：直接允许退出。如果有需要异步完成的清理工作，应在此处执行，
                // 并在清理完成后手动调用 std::process::exit(0)。
                // 为确保所有资源得到释放，特别是如果 ws_service 有后台任务，
                // 可能需要在此处优雅地关闭这些任务。
                // 例如：_app_handle.state::<Arc<WebSocketClientService>>().disconnect().await;
                info!("正在执行退出前的清理操作 (如果需要)... (SatOnSiteMobile)");
                // 这里我们简单地直接退出进程
                std::process::exit(0);
            }
            tauri::RunEvent::Exit => {
                // 当应用实际退出时触发
                info!("Tauri 运行时事件：应用即将退出 (SatOnSiteMobile)。");
            }
            // 可以根据需要处理其他 tauri::RunEvent，例如 WindowEvent 等
            _ => { /* 忽略其他未明确处理的运行时事件 */ }
        });
    // .run() 方法会阻塞当前线程直到应用退出。
    // 因此，这条日志通常在应用关闭后才可能（在某些情况下）被记录，或者根本不会执行到。
    info!("Tauri 应用的 run 方法已结束调用，应用已关闭 (SatOnSiteMobile)。");
}
