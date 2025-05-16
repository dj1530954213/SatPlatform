use log::{error, info, LevelFilter};
use servertest::ws_server::service::WsService;
use servertest::ws_server::connection_manager::ConnectionManager;
use servertest::ws_server::task_state_manager::TaskStateManager;
use servertest::ws_server::heartbeat_monitor::HeartbeatMonitor;
use std::sync::Arc;
use std::time::Duration;
use servertest::config::WebSocketConfig;

#[tokio::main]
async fn main() {
    // 初始化日志记录器
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();
    info!("[主程序] 日志系统已成功初始化 (env_logger)，默认级别: Info。");

    // 使用硬编码的WebSocket配置
    let ws_config = WebSocketConfig {
        host: "0.0.0.0".to_string(),
        port: 8088,
        heartbeat_check_interval_seconds: 15,
        client_timeout_seconds: 60,
    };
    
    // 初始化应用配置（仅用于其他可能的配置）
    servertest::config::init_config();
    let app_config = servertest::config::get_config();
    info!("[主程序] 应用配置已加载。WebSocket服务使用硬编码地址: 0.0.0.0:8088");

    // 创建任务状态管理器
    let task_state_manager = Arc::new(TaskStateManager::new());
    info!("[主程序] 任务状态管理器 (TaskStateManager) 已创建。");

    // 创建连接管理器
    let connection_manager = Arc::new(ConnectionManager::new(task_state_manager.clone()));
    info!("[主程序] WebSocket 连接管理器 (ConnectionManager) 已创建，并已注入任务状态管理器。");

    // 为 WebSocket 服务创建一个新的 WsService 实例，使用硬编码配置
    let ws_service_instance = WsService::new(
        ws_config, 
        connection_manager.clone(),
        task_state_manager.clone(),
    );
    
    // 创建心跳监视器，使用硬编码的心跳配置
    let heartbeat_check_interval = Duration::from_secs(15);
    let client_timeout = Duration::from_secs(60);
    let heartbeat_monitor = HeartbeatMonitor::new(
        connection_manager.clone(),
        client_timeout,
        heartbeat_check_interval,
    );

    // 启动心跳监视器
    tokio::spawn(async move {
        info!("[主程序] 正在创建并启动独立的心跳监视器 (HeartbeatMonitor) 异步任务...");
        heartbeat_monitor.run().await;
        info!("[主程序] 警告：心跳监视器 (HeartbeatMonitor) 任务已意外结束。这可能表明存在问题。");
    });
    info!("[主程序] 心跳监视器 (HeartbeatMonitor) 启动任务已成功派生到后台异步执行。");

    // 启动 WebSocket 服务
    info!("[主程序] 正在启动 WebSocket 服务...");
    if let Err(e) = ws_service_instance.start().await {
        error!("[主程序] 致命错误：启动 WebSocket 服务时发生严重问题: {}", e);
    }
}
