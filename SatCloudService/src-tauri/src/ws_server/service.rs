// SatCloudService/src-tauri/src/ws_server/service.rs

//! WebSocket 服务端核心服务，如启动监听等。

// 暂时为空，后续会根据 P1.1.1 添加 start_ws_server 函数等。 

use crate::config::WsConfig;
use anyhow::Context;
use std::sync::Arc;
use tauri::AppHandle;
use crate::ws_server::connection_manager::ConnectionManager;
use rust_websocket_utils::message::WsMessage as ActualWsMessage; // 修改这里

// 以下是 `rust_websocket_utils` 和 `common_models` 中实际类型的占位符。
// 这些类型会根据项目规范在相应的 crate 中定义。
// 例如: WsMessage 对应 common_models::ws::WsMessage
// 例如: ClientId 对应 uuid::Uuid

// 这是一个简化的占位符模块，用于模拟 `rust_websocket_utils` 可能提供的服务端功能。
// 实际的 API 将取决于 P0.3.x 阶段中 `rust_websocket_utils` 的具体实现。
mod hypothetical_ws_utils {
    use anyhow::Result;
    use std::net::SocketAddr;
    use tokio::sync::mpsc;
    use uuid::Uuid;
    use super::ActualWsMessage; // 使用外部定义的 ActualWsMessage

    // hypothetical_ws_utils::WsMessage 这个定义可以移除，或者保留但不在回调签名中使用
    // #[derive(Debug)]
    // pub struct WsMessage { pub content: String } 

    pub struct ServerTransportLayer;

    impl ServerTransportLayer {
        pub async fn serve(
            config: crate::config::WsConfig,
            mut on_connect: impl FnMut(Uuid, SocketAddr, mpsc::Sender<ActualWsMessage>) + Send + Sync + 'static,
            _on_message: impl FnMut(Uuid, ActualWsMessage) + Send + Sync + 'static, 
            _on_disconnect: impl FnMut(Uuid) + Send + Sync + 'static, 
        ) -> Result<()> {
            let addr = format!("{}:{}", config.host, config.port);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            log::info!("[ws_server] WebSocket 服务正在监听: {}", addr);

            loop {
                match listener.accept().await {
                    Ok((stream, client_addr)) => {
                        let client_id = Uuid::new_v4();
                        log::info!("[ws_server] 新客户端尝试连接: ID: {}, 地址: {}", client_id, client_addr);
                        
                        match tokio_tungstenite::accept_async(stream).await {
                            Ok(_ws_stream) => { 
                                log::info!("[ws_server] 客户端 WebSocket 握手成功: ID: {}, 地址: {}", client_id, client_addr);
                                // 创建的 channel 必须是 mpsc::channel::<ActualWsMessage>
                                let (tx, _rx) = mpsc::channel::<ActualWsMessage>(32);
                                on_connect(client_id, client_addr, tx.clone());
                                tokio::spawn(async move {});
                            }
                            Err(e) => {
                                log::error!("[ws_server] 客户端 {} 的 WebSocket 握手错误: {}", client_addr, e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("[ws_server] 接受传入连接失败: {}", e);
                    }
                }
            }
        }
    }
}

/// 启动 WebSocket 服务端。
///
/// # 参数
/// * `app_handle`: Tauri 应用的句柄，可用于访问应用配置等。
///
/// # 返回
/// * `Result<(), anyhow::Error>`: 如果服务成功启动并运行（或按预期设计为有限运行后退出），则返回 `Ok(())`。
///   如果启动过程中发生错误，则返回包含详细错误信息的 `Err`。
pub async fn start_ws_server(app_handle: AppHandle) -> Result<(), anyhow::Error> {
    log::info!("[ws_server] 正在初始化 WebSocket 服务...");

    let ws_config = Arc::new(WsConfig::load(&app_handle));
    log::info!(
        "[ws_server] 配置已加载: host={}, port={}",
        ws_config.host,
        ws_config.port
    );

    // 创建 ConnectionManager
    let connection_manager = Arc::new(ConnectionManager::new());

    // 定义 on_connect 回调
    let conn_manager_clone_on_connect = Arc::clone(&connection_manager);
    let on_connect_cb = move |_client_id_from_util: uuid::Uuid, addr: std::net::SocketAddr, sender: tokio::sync::mpsc::Sender<ActualWsMessage>| {
        let manager = Arc::clone(&conn_manager_clone_on_connect);
        tokio::spawn(async move {
            let _client_session = manager.add_client(addr, sender).await;
        });
    };

    let on_message_cb = move |client_id: uuid::Uuid, message: ActualWsMessage| {
        log::info!("[ws_server] [占位回调] 来自客户端 {} 的消息 - 内容: {:?}", client_id, message);
    };

    let on_disconnect_cb = move |client_id: uuid::Uuid| {
        log::info!("[ws_server] [占位回调] 客户端已断开 - ID: {}", client_id);
        // 后续操作: P1.2.2 调用 ConnectionManager.remove_client(client_id)
    };

    if let Err(e) = hypothetical_ws_utils::ServerTransportLayer::serve(
        (*ws_config).clone(),
        on_connect_cb,
        on_message_cb,
        on_disconnect_cb,
    ).await {
        log::error!("[ws_server] WebSocket 服务启动失败或遇到致命错误: {}", e);
        return Err(e).context("WebSocket 服务运行失败");
    }
    
    log::warn!("[ws_server] WebSocket 服务意外停止。");
    Ok(())
} 