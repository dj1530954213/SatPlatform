// SatCloudService/src-tauri/src/main.rs

use tauri::Manager; // 新增：导入 Manager trait 以使用 app.manage()
// use tauri::Manager; // 暂时注释掉未使用的导入，后续如需使用应用句柄可取消注释
use log::{error, info, LevelFilter}; // 引入日志宏 (error, info) 和日志级别过滤器 (LevelFilter)
use sat_cloud_service::ws_server::service::WsService; // 引入 WebSocket 服务实现
use sat_cloud_service::ws_server::connection_manager::ConnectionManager; // 引入 WebSocket 连接管理器
use sat_cloud_service::ws_server::task_state_manager::TaskStateManager; // P3.1.2: 引入任务状态管理器，用于管理调试任务的共享状态
use sat_cloud_service::ws_server::heartbeat_monitor::HeartbeatMonitor; // P3.2.1: 引入心跳监视器，用于检测和处理客户端超时
use std::sync::Arc; // 引入原子引用计数 Arc，用于在多线程环境安全地共享状态所有权
use std::time::Duration; // P3.2.1: 引入时间间隔 Duration，用于定义超时和检查周期

// 此配置用于在 Windows 平台的发布 (release) 构建中阻止显示一个额外的控制台窗口。
// 请勿移除此行，这对于提升 Windows 用户体验很重要！
#[cfg_attr(
    all(not(debug_assertions), target_os = "windows"), // 条件编译：当不是调试模式 (debug_assertions=false) 并且目标操作系统是 Windows 时生效
    windows_subsystem = "windows" // 设置 Windows 子系统为 "windows"，隐藏控制台
)]

// 关于 Tauri 命令的更多详细信息和用法，请参考官方文档：
// https://tauri.app/v1/guides/features/command (旧版 v1 链接，v2 请查阅 v2.tauri.app)
// 这是一个简单的 Tauri 命令示例，用于演示从前端调用 Rust 函数。
#[tauri::command] // 宏，将此函数声明为一个可从前端调用的 Tauri 命令
fn greet(name: &str) -> String {
    // 使用 format! 宏构建一个包含问候信息的字符串
    // 例如，如果 name 是 "World"，则返回 "你好, World! 来自 Rust 的问候!"
    format!("你好, {}! 来自 Rust 的问候!", name)
}

// 声明应用配置模块，该模块负责加载和管理应用的配置信息
// mod config; // 已在 lib.rs 中声明，此处无需重复，但保留注释以作说明
// 声明 WebSocket 服务端相关模块的父模块
// mod ws_server; // 已在 lib.rs 中声明

// Rust 程序的主函数，Tauri 应用的入口点。
fn main() {
    // 初始化 `env_logger` 日志记录器。
    // `env_logger` 是一个常用的日志实现，可以通过环境变量 (如 RUST_LOG) 配置日志级别。
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info) // 设置默认的日志过滤级别为 Info (即 Info, Warn, Error 级别的日志都会被记录)
        .format_timestamp_millis()       // 在每个日志条目的时间戳中包含毫秒，以便更精确地追踪事件顺序
        .init(); // 完成日志记录器的初始化，并将其注册为全局日志处理器
    info!("[主程序] 日志系统已成功初始化 (env_logger)，默认级别: Info。");

    // 构建并启动 Tauri 应用程序实例。
    tauri::Builder::default() // 创建一个默认的 Tauri 应用构建器
        .setup(|app| { // `setup` 是一个钩子函数，它在 Tauri 应用的核心部分（如窗口）创建之前执行
            info!("[主程序::Setup钩子] 开始执行 Tauri 应用的初始化设置...");

            // 初始化应用配置模块。这通常涉及到加载配置文件或使用默认配置。
            // 需要传递 `app.handle()` (应用句柄)，因为它可能需要访问应用特定的路径信息。
            sat_cloud_service::config::init_config(&app.handle());
            let app_config = sat_cloud_service::config::get_config(); // 获取已加载并缓存的全局应用配置
            info!("[主程序::Setup钩子] 应用配置已成功加载。配置详情: {:?}", app_config);

            // P3.1.2: 创建任务状态管理器 (TaskStateManager) 的实例。
            // TaskStateManager 负责管理所有活动调试任务的共享状态数据。
            // 使用 Arc 包装，以便在多个组件（如 ConnectionManager 和 MessageRouter）之间安全地共享所有权。
            let task_state_manager = Arc::new(TaskStateManager::new());
            info!("[主程序::Setup钩子] 任务状态管理器 (TaskStateManager) 已创建。");

            // P1.2.1 & P3.1.2: 创建连接管理器 (ConnectionManager) 的实例。
            // ConnectionManager 负责管理所有 WebSocket 客户端连接、会话、组等。
            // 将 task_state_manager 的 Arc 克隆并注入到 ConnectionManager 中，使其能够访问和修改任务状态。
            let connection_manager = Arc::new(ConnectionManager::new(task_state_manager.clone()));
            info!("[主程序::Setup钩子] WebSocket 连接管理器 (ConnectionManager) 已创建，并已注入任务状态管理器。");

            // P1.2.1: 将 ConnectionManager 的 Arc 引用放入 Tauri 的托管状态 (Managed State) 中。
            // 这样做之后，就可以在 Tauri 的命令 (command) 处理函数中通过类型签名请求并访问 ConnectionManager 实例。
            app.manage(connection_manager.clone()); 
            info!("[主程序::Setup钩子] WebSocket 连接管理器 (ConnectionManager) 已成功添加到 Tauri 托管状态。");
            
            // P3.3.1: 将 TaskStateManager 的 Arc 引用也放入 Tauri 的托管状态中。
            // 这样，如果需要在 Tauri 命令中直接操作 TaskStateManager (例如，从一个HTTP API触发状态变更)，也可以做到。
            app.manage(task_state_manager.clone());
            info!("[主程序::Setup钩子] 任务状态管理器 (TaskStateManager) 已成功添加到 Tauri 托管状态。");


            // 为 WebSocket 服务创建一个新的 WsService 实例。
            // 它需要应用的 WebSocket 配置 (从 app_config 中获取) 和对 ConnectionManager 的共享引用。
            let ws_service_instance = WsService::new(app_config.websocket.clone(), connection_manager.clone());
            
            // 使用 Tauri 的异步运行时 (tauri::async_runtime::spawn) 在后台启动 WebSocket 服务。
            // 这是一个独立的异步任务，不会阻塞 setup 钩子或主线程。
            tauri::async_runtime::spawn(async move {
                info!("[主程序::Setup钩子] 正在创建并启动独立的 WebSocket 服务异步任务...");
                // 调用 WsService 的 start 方法来启动监听和接受连接的循环。
                if let Err(e) = ws_service_instance.start().await { 
                    // 如果 start 方法返回错误 (例如，端口已被占用或绑定失败)，则记录严重错误。
                    error!("[主程序::Setup钩子] 致命错误：启动 WebSocket 服务时发生严重问题: {}", e);
                }
                // 注意：如果 start() 正常结束（例如被外部信号中断），此异步任务也会结束。
                // 在一个持续运行的服务中，start() 通常是一个无限循环，除非出错或被明确停止。
            });
            info!("[主程序::Setup钩子] WebSocket 服务启动任务已成功派生到后台异步执行。");

            // P3.2.1: 从已加载的应用配置中读取心跳检查间隔和客户端超时时间。
            let heartbeat_check_interval = Duration::from_secs(app_config.websocket.heartbeat_check_interval_seconds);
            let client_timeout = Duration::from_secs(app_config.websocket.client_timeout_seconds);
            
            // 创建心跳监视器 (HeartbeatMonitor) 的实例。
            // 它需要对 ConnectionManager 的共享引用 (以便移除超时的客户端)，以及配置的超时参数。
            let heartbeat_monitor = HeartbeatMonitor::new(
                connection_manager.clone(), // 传递 ConnectionManager 的 Arc 引用
                client_timeout,             // 设置客户端不活跃的超时时长
                heartbeat_check_interval,   // 设置心跳监视器自身的检查频率
            );
            // 使用 Tauri 的异步运行时在后台启动心跳监视器。
            // 这也是一个独立的异步任务。
            tauri::async_runtime::spawn(async move {
                info!("[主程序::Setup钩子] 正在创建并启动独立的心跳监视器 (HeartbeatMonitor) 异步任务...");
                heartbeat_monitor.run().await; // 运行心跳监视器的主循环，它会定期检查客户端的活跃状态
                // run() 方法通常也是一个无限循环。如果它结束了，说明心跳监视器停止了工作，这可能是一个异常情况。
                info!("[主程序::Setup钩子] 警告：心跳监视器 (HeartbeatMonitor) 任务已意外结束。这可能表明存在问题。");
            });
            info!("[主程序::Setup钩子] 心跳监视器 (HeartbeatMonitor) 启动任务已成功派生到后台异步执行。");

            info!("[主程序::Setup钩子] Tauri 应用的所有初始化设置已全部完成。");
            Ok(()) // 表示 setup 钩子成功完成
        })
        .invoke_handler(tauri::generate_handler![greet]) // 注册 Tauri 命令处理器，使得 `greet` 命令可以从前端调用
        .run(tauri::generate_context!()) // 启动并运行 Tauri 应用，传入通过宏在编译时生成的应用上下文信息
        .expect("[主程序] 致命错误：运行 Tauri 应用时发生不可恢复的问题，应用无法启动。"); // 如果 run 方法返回错误，程序将 panic 并显示此消息
}
