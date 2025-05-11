// SatCloudService/src-tauri/src/ws_server/service.rs

//! WebSocket 服务端核心服务，如启动监听等。

// 暂无特定内容，P1.1.1 中已添加 start_ws_server 函数。

use crate::config::WebSocketConfig; // 使用 WebSocketConfig 而不是 WsConfig
use crate::ws_server::connection_manager::ConnectionManager;
use crate::ws_server::message_router; // 导入 message_router 模块
use anyhow::{Context, Result}; // 确保 Context 被导入
use futures_util::stream::SplitStream;
use log::{debug, error, info, warn};
use rust_websocket_utils::{
    message::WsMessage as ActualWsMessage,
    server::transport::{
        start_server,
        ConnectionHandler as WsConnectionHandler,
        receive_message,
    },
    error::WsError,
};
use std::sync::Arc;
// use tauri::AppHandle; // AppHandle 不再作为 start_ws_server 的参数
use tokio::sync::mpsc;
// 旧: use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage; // -- 警告：未使用
use tokio_tungstenite::WebSocketStream;
use tokio::net::TcpStream;

/// WebSocket 服务结构体，封装了配置和连接管理器。
pub struct WsService {
    config: WebSocketConfig, // WebSocketSpecific配置
    connection_manager: Arc<ConnectionManager>,
}

impl WsService {
    /// 创建一个新的 WsService 实例。
    pub fn new(config: WebSocketConfig, connection_manager: Arc<ConnectionManager>) -> Self {
        info!("[WsService] New instance created.");
        Self {
            config,
            connection_manager,
        }
    }

    /// 启动 WebSocket 服务端。
    pub async fn start(&self) -> Result<(), anyhow::Error> {
        info!("[WsService] Starting WebSocket service...");
        info!(
            "[WsService] Configuration: host={}, port={}",
            self.config.host, self.config.port
        );

        // ConnectionManager 和 TaskStateManager 的创建移至 main.rs
        // 这里直接使用传入的 connection_manager

        let on_new_connection_cb = {
            let conn_manager_for_cb = Arc::clone(&self.connection_manager);
            
            move |ws_conn_handler: WsConnectionHandler, mut ws_receiver: SplitStream<WebSocketStream<TcpStream>>| {
                let connection_manager_clone = Arc::clone(&conn_manager_for_cb);
                
                async move {
                    let (tx_to_client_session, mut rx_from_client_session) = mpsc::channel::<ActualWsMessage>(32);
                    
                    // TODO: [rust_websocket_utils enhancement]
                    // ws_conn_handler 应该提供获取客户端地址的方法。
                    // let actual_addr = ws_conn_handler.addr(); 
                    // 临时使用占位符地址，直到 ws_conn_handler.addr() 可用。
                    let actual_addr: std::net::SocketAddr = "0.0.0.0:0".parse().expect("Failed to parse placeholder SocketAddr");
                    warn!("[WsService] Using placeholder address for new client: {}. Update rust_websocket_utils::ConnectionHandler to provide actual address.", actual_addr);

                    // TODO: [rust_websocket_utils enhancement]
                    // ws_conn_handler 应该提供获取连接关闭句柄的方法。
                    // let close_handle = ws_conn_handler.get_close_handle();
                    // 临时创建一个新的 close_handle，直到 ws_conn_handler.get_close_handle() 可用。
                    // 这意味着通过 ConnectionManager::remove_client 主动关闭连接可能不会立即关闭物理链路，
                    // 除非 rust_websocket_utils 的 ConnectionHandler 内部也监听这个。
                    // 理想情况下，这个 close_handle 应该是由 rust_websocket_utils 创建并传递出来的。
                    let close_handle = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    warn!("[WsService] Using a locally created close_handle for new client. Timeout-based disconnection might not close physical link as expected until rust_websocket_utils::ConnectionHandler provides its own handle.");

                    // 调用 add_client 时传递真实的 addr 和 close_handle
                    let client_session = connection_manager_clone.add_client(
                        actual_addr, 
                        tx_to_client_session, 
                        close_handle
                    ).await;
                    
                    info!(
                        "[WsService] New client connected: SessionID={}, Addr={:?}",
                        client_session.client_id,
                        client_session.addr
                    );

                    let client_session_id_for_sender_task = client_session.client_id;
                    // ws_conn_handler is moved into sender_task_ws_conn_handler_owner
                    // so it can be dropped when the sender task truly finishes.
                    let sender_task_ws_conn_handler_owner = ws_conn_handler; 
                    let client_session_for_sender_task = Arc::clone(&client_session);

                    let sender_task_join_handle = tokio::spawn(async move {
                        // The WsConnectionHandler is now owned by this task.
                        let mut sender_task_ws_conn_handler = sender_task_ws_conn_handler_owner;
                        loop { // Modified to loop and check close signal
                            // Check close signal before trying to receive a message
                            if client_session_for_sender_task.connection_should_close.load(std::sync::atomic::Ordering::SeqCst) {
                                info!(
                                    "[SenderTask {}] Close signal received, sender task ending.",
                                    client_session_id_for_sender_task
                                );
                                break;
                            }

                            tokio::select! {
                                biased;
                                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                                    // Periodically check the close signal if recv() is blocking for too long
                                    // This also prevents a tight loop if recv() immediately returns None often
                                    continue;
                                }
                                maybe_msg_to_send = rx_from_client_session.recv() => {
                                    if let Some(ws_msg_to_send) = maybe_msg_to_send {
                                        debug!(
                                            "[SenderTask {}] Sending message: Type={}",
                                            client_session_id_for_sender_task, ws_msg_to_send.message_type
                                        );
                                        if sender_task_ws_conn_handler.send_message(&ws_msg_to_send).await.is_err() {
                                            error!(
                                                "[SenderTask {}] Failed to send message to client. Assuming connection is broken.",
                                                client_session_id_for_sender_task
                                            );
                                            // Optionally, also signal to close from here if send fails, though usually receiver side handles this.
                                            // client_session_for_sender_task.connection_should_close.store(true, std::sync::atomic::Ordering::SeqCst);
                                            break; // Exit loop on send error
                                        }
                                    } else {
                                        info!(
                                            "[SenderTask {}] Channel closed (rx_from_client_session returned None), sender task ending.",
                                            client_session_id_for_sender_task
                                        );
                                        break; // Exit loop if channel is closed
                                    }
                                }
                            }
                        }
                        debug!("[SenderTask {}] Loop exited, sender task fully ended.", client_session_id_for_sender_task);
                        // sender_task_ws_conn_handler is dropped here as the task ends.
                    });
                    
                    let client_session_clone_for_router = Arc::clone(&client_session);
                    let connection_manager_for_router = Arc::clone(&connection_manager_clone);
                    loop {
                        // Check close signal before trying to receive a message
                        if client_session_clone_for_router.connection_should_close.load(std::sync::atomic::Ordering::SeqCst) {
                            info!(
                                "[WsService] SessionID {}: Close signal received in receiver loop. Terminating connection handling.",
                                client_session_clone_for_router.client_id
                            );
                            break;
                        }

                        // Use tokio::select! to make receive_message non-blocking with a timeout
                        // This allows the loop to periodically check connection_should_close
                        let mut receive_fut = Box::pin(receive_message(&mut ws_receiver));
                        let received_result = tokio::select! {
                            biased;
                            res = &mut receive_fut => Some(res), // Captures Option<Result<WsMessage, WsError>>
                            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => None, // Timeout, outer Option is None
                        };

                        match received_result { // received_result is Option<Option<Result<WsMessage, WsError>>>
                            Some(Some(Ok(ws_msg))) => { // Inner Option is Some, Result is Ok
                                if let Err(e) = message_router::handle_message(
                                    Arc::clone(&client_session_clone_for_router),
                                    ws_msg,
                                    Arc::clone(&connection_manager_for_router),
                                )
                                .await
                                {
                                    error!(
                                        "[WsService] SessionID {}: Error handling message: {}",
                                        client_session_clone_for_router.client_id, e
                                    );
                                }
                            }
                            Some(Some(Err(ws_err))) => { // Inner Option is Some, Result is Err
                                match ws_err {
                                    WsError::DeserializationError(e) => {
                                        warn!(
                                            "[WsService] SessionID {}: Message deserialization failed: {}. Notifying client if possible.",
                                            client_session_clone_for_router.client_id, e
                                        );
                                        // This might not be a fatal error for the connection itself, so don't break yet.
                                    }
                                    WsError::WebSocketProtocolError(e) => {
                                        warn!(
                                            "[WsService] SessionID {}: WebSocket protocol error: {}. Connection likely broken.",
                                            client_session_clone_for_router.client_id, e
                                        );
                                        break; // Protocol error usually means connection is dead.
                                    }
                                    WsError::Message(s) => { 
                                        warn!(
                                            "[WsService] SessionID {}: Internal message error: {}. Connection likely broken.",
                                            client_session_clone_for_router.client_id, s
                                        );
                                         break; // Assume internal errors are serious enough to break.
                                    }
                                    _ => { // Other WsError types from rust_websocket_utils
                                        error!(
                                            "[WsService] SessionID {}: Unhandled WsError from receive_message: {:?}. Disconnecting.",
                                            client_session_clone_for_router.client_id, ws_err
                                        );
                                        break;
                                    }
                                }
                            }
                            Some(None) => { // Inner Option is None, meaning receive_message itself indicated graceful peer closure.
                                info!(
                                    "[WsService] SessionID {}: Client connection closed by peer (receive_message returned None).",
                                    client_session_clone_for_router.client_id
                                );
                                break;
                            }
                            None => { // Outer Option is None, meaning tokio::select! timed out.
                                // Timeout occurred, loop again to check close signal and try receiving again.
                                debug!("[WsService] SessionID {}: Receive message timed out, checking close signal and retrying.", client_session_clone_for_router.client_id);
                                // No action needed here, loop will continue and re-check close signal / re-attempt receive.
                                continue; 
                            }
                        }
                    }

                    info!(
                        "[WsService] SessionID {}: Connection handling task (receiver loop) ended. Waiting for sender task to complete...",
                        client_session.client_id
                    );

                    // Wait for the sender task to complete. 
                    // This ensures its resources (like its WsConnectionHandler copy) are dropped.
                    if let Err(e) = sender_task_join_handle.await {
                        error!(
                            "[WsService] SessionID {}: Sender task panicked or encountered an error during join: {:?}",
                            client_session.client_id, e
                        );
                    } else {
                        info!(
                            "[WsService] SessionID {}: Sender task completed and joined successfully.",
                            client_session.client_id
                        );
                    }
                    
                    // Now that both sender and receiver loops are confirmed to be finished,
                    // and their respective parts of the WebSocket connection (handler/stream)
                    // should have been dropped, we proceed with the logical removal from ConnectionManager.

                    info!(
                        "[WsService] SessionID {}: All send/receive loops for connection have ended. Proceeding with final cleanup in ConnectionManager...",
                        client_session.client_id
                    );
                    let role_before_remove = client_session.role.read().await.clone();
                    let group_id_before_remove = client_session.group_id.read().await.clone();

                    connection_manager_clone.remove_client(&client_session.client_id).await;
                    info!(
                        "[WsService] Session {} (Addr: {:?}, Role at disconnect: {:?}, Group at disconnect: {:?}) cleanup initiated via ConnectionManager::remove_client.",
                        client_session.client_id,
                        client_session.addr,
                        role_before_remove,
                        group_id_before_remove.as_deref().unwrap_or("N/A")
                    );
                }
            }
        };
        
        let listen_addr = format!("{}:{}", self.config.host, self.config.port);

        if let Err(e) = start_server(listen_addr, on_new_connection_cb).await {
            error!("[WsService] Failed to start WebSocket server or critical error during operation: {}", e);
            return Err(anyhow::Error::from(e)).context("WebSocket server operation failed");
        }
        
        warn!("[WsService] WebSocket server has unexpectedly stopped. The start_server function returned.");
        Ok(())
    }
} 