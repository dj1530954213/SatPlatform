// SatCloudService/src-tauri/src/ws_server/service.rs

//! WebSocket 服务端核心服务，如启动监听等。

// 暂无特定内容，P1.1.1 中已添加 start_ws_server 函数。

use crate::config::WsConfig;
// 旧: use crate::ws_server::client_session::ClientSession; // 确保 ClientSession 已导入 -- 警告：未使用
use crate::ws_server::connection_manager::ConnectionManager;
use crate::ws_server::message_router; // 导入 message_router 模块
use crate::ws_server::task_state_manager::TaskStateManager; // P3.1.2 新增
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
use tauri::AppHandle;
use tokio::sync::mpsc;
// 旧: use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage; // -- 警告：未使用
use tokio_tungstenite::WebSocketStream;
use tokio::net::TcpStream;

/// 启动 WebSocket 服务端。
///
/// # 参数
/// * `app_handle`: Tauri 应用的句柄，可用于访问应用配置等。
///
/// # 返回
/// * `Result<(), anyhow::Error>`: 如果服务成功启动并运行（或按预期设计为有限运行后退出），则返回 `Ok(())`。
///   如果启动过程中发生错误，则返回包含详细错误信息的 `Err`。
pub async fn start_ws_server(app_handle: AppHandle) -> Result<(), anyhow::Error> {
    info!("[WebSocket服务] 正在初始化WebSocket服务...");

    let ws_config = Arc::new(WsConfig::load(&app_handle));
    info!(
        "[WebSocket服务] 配置已加载: host={}, port={}",
        ws_config.host, ws_config.port
    );

    // P3.1.2: 创建 TaskStateManager
    let task_state_manager = Arc::new(TaskStateManager::new());
    info!("[WebSocket服务] TaskStateManager (骨架) 已创建。");

    // P3.1.2: 修改 ConnectionManager 创建，传入 task_state_manager
    let connection_manager = Arc::new(ConnectionManager::new(Arc::clone(&task_state_manager)));
    info!("[WebSocket服务] ConnectionManager 已创建并注入 TaskStateManager。");

    // 定义 on_new_connection 回调
    // 此回调会在每个新的 WebSocket 连接成功建立后被调用。
    let on_new_connection_cb = {
        let conn_manager_for_cb = Arc::clone(&connection_manager);
        
        // Clone AppHandle for potential future use inside the callback if needed (e.g. for state or events)
        // let _app_handle_for_cb = app_handle.clone(); 

        move |mut ws_conn_handler: WsConnectionHandler, mut ws_receiver: SplitStream<WebSocketStream<TcpStream>>| {
            // 对每个连接，都克隆一次 Arc<ConnectionManager>
            let connection_manager_clone = Arc::clone(&conn_manager_for_cb);
            
            // 为每个连接创建一个独立的 Tokio 任务来处理其生命周期
            async move {
                // 1. 为此连接创建一个 mpsc channel，用于将待发送的 WsMessage 从 ClientSession 传递到实际的发送逻辑
                //    ClientSession 将持有 tx，此任务将持有 rx 并使用 ws_conn_handler.send_message 发送。
                let (tx_to_client_session, mut rx_from_client_session) = mpsc::channel::<ActualWsMessage>(32); // 32 是通道缓冲区大小

                // 2. 使用 ConnectionManager 创建 ClientSession
                //    注意：add_client 需要 SocketAddr，但 WsConnectionHandler 和 SplitStream 目前不直接提供。
                //    这是一个需要解决的差异。 rust_websocket_utils::server::transport::start_server
                //    在接受连接时有 client_addr，但它没有直接传递给 on_new_connection 回调。
                //    需要修改 rust_websocket_utils::server::transport::start_server 让它传递 client_addr。
                //    【临时占位】：使用一个虚拟的 SocketAddr。这需要在 P0.3 或此处进一步修订。
                let temp_placeholder_addr: std::net::SocketAddr = "0.0.0.0:0".parse().unwrap();
                // TODO: 确保 rust_websocket_utils::server::transport::start_server 将 SocketAddr 传递给 on_new_connection 回调
                //       并在此处使用真实的 client_addr。

                let client_session = connection_manager_clone.add_client(temp_placeholder_addr, tx_to_client_session).await;
                info!(
                    "[WebSocket服务] 新客户端已连接并注册: SessionID={}, Addr={:?}", // Addr 之后会是真实的
                    client_session.client_id,
                    client_session.addr // 将显示占位符地址
                );

                // 3. 启动一个任务，专门负责从 rx_from_client_session 读取消息并使用 ws_conn_handler 发送
                let client_session_id_for_sender_task = client_session.client_id;
                let mut sender_task_ws_conn_handler = ws_conn_handler; // ws_conn_handler 需要被此任务拥有

                tokio::spawn(async move {
                    while let Some(ws_msg_to_send) = rx_from_client_session.recv().await {
                        debug!(
                            "[SenderTask {}] 从通道收到消息，准备通过 WebSocket 发送: Type={}",
                            client_session_id_for_sender_task, ws_msg_to_send.message_type
                        );
                        if let Err(e) = sender_task_ws_conn_handler.send_message(&ws_msg_to_send).await {
                            error!(
                                "[SenderTask {}] 发送消息到客户端失败: {}",
                                client_session_id_for_sender_task, e
                            );
                            // 如果发送失败（例如连接已断开），可以考虑关闭 mpsc channel 的接收端
                            // 以便任何尝试发送到此 ClientSession 的地方能感知到。
                            // 但通常，如果 WebSocket 连接断开，下面的接收循环会先结束。
                        }
                    }
                    debug!("[SenderTask {}] MPSC 通道已关闭，发送任务结束。", client_session_id_for_sender_task);
                });
                
                // 4. 主循环：接收来自客户端的消息，并交由 MessageRouter 处理
                let client_session_clone_for_router = Arc::clone(&client_session);
                // P3.1.2: 克隆 ConnectionManager 以传递给 handle_message
                let connection_manager_for_router = Arc::clone(&connection_manager_clone);
                loop {
                    match receive_message(&mut ws_receiver).await {
                        Some(Ok(ws_msg)) => {
                            // 将接收到的消息和 ClientSession 交给 MessageRouter 处理
                            if let Err(e) = message_router::handle_message(
                                Arc::clone(&client_session_clone_for_router),
                                ws_msg,
                                Arc::clone(&connection_manager_for_router), // P3.1.2: 传递 CM
                            )
                            .await
                            {
                                error!(
                                    "[WebSocket服务] SessionID {}: 处理消息时发生错误: {}",
                                    client_session_clone_for_router.client_id, e
                                );
                                // 根据错误类型决定是否需要断开连接或采取其他措施
                                // 例如，某些错误可能是致命的，需要关闭此客户端连接
                            }
                        }
                        Some(Err(ws_err)) => {
                            // 处理从 receive_message 返回的错误
                            match ws_err {
                                WsError::DeserializationError(e) => {
                                    warn!(
                                        "[WebSocket服务] SessionID {}: 接收消息反序列化失败: {}。可能需要通知客户端。",
                                        client_session_clone_for_router.client_id, e
                                    );
                                    // 可以选择向客户端发送一个格式错误的响应，但这较难，因为原始消息可能无法解析
                                    // 通常，handle_message 内部的 ErrorResponse 机制更适合处理可解析但无效的 payload
                                }
                                WsError::WebSocketProtocolError(e) => {
                                    warn!(
                                        "[WebSocket服务] SessionID {}: WebSocket 协议错误: {}。连接可能已损坏。",
                                        client_session_clone_for_router.client_id, e
                                    );
                                    break; // 协议错误通常意味着连接不再可用
                                }
                                WsError::Message(s) => { // 一般消息错误
                                    warn!(
                                        "[WebSocket服务] SessionID {}: 内部消息处理错误: {}. 连接可能已损坏。",
                                        client_session_clone_for_router.client_id, s
                                    );
                                     break;
                                }
                                _ => { // 其他 WsError 类型 (IoError, SerializationError (发送时), etc.)
                                    error!(
                                        "[WebSocket服务] SessionID {}: 从 receive_message 收到未明确处理的 WsError: {:?}。断开连接。",
                                        client_session_clone_for_router.client_id, ws_err
                                    );
                                    break;
                                }
                            }
                        }
                        None => {
                            // receive_message 返回 None 表示连接已正常关闭或流结束
                            info!(
                                "[WebSocket服务] SessionID {}: 客户端连接已关闭 (由 receive_message 返回 None)。",
                                client_session_clone_for_router.client_id
                            );
                            break; // 退出接收循环
                        }
                    }
                }

                // 5. 连接结束后，清理工作
                info!(
                    "[WebSocket服务] SessionID {}: 连接处理任务结束。正在从 ConnectionManager 移除会话...",
                    client_session.client_id
                );
                // 在移除客户端前，先获取其角色和组ID的最新状态，用于日志记录
                let role_before_remove = client_session.role.read().await.clone();
                let group_id_before_remove = client_session.group_id.read().await.clone();

                connection_manager_clone.remove_client(&client_session.client_id).await;
                // 日志记录现在将依赖于 remove_client 内部的日志，因为它能访问到最新的组信息。
                info!(
                    "[WebSocket服务] 会话 {} (Addr: {:?}, Role at disconnect: {:?}, Group at disconnect: {:?}) 的清理流程已调用 ConnectionManager::remove_client。",
                    client_session.client_id,
                    client_session.addr,
                    role_before_remove,
                    group_id_before_remove.as_deref().unwrap_or("N/A")
                );
                // 发送任务的 rx_from_client_session 会因为其对应的 tx (在 ClientSession 中) 被 drop 而结束。
                // 或者可以显式地 drop/close client_session.sender 中的 tx，但这需要 ClientSession 内部做更改。
                // 这里依赖 ClientSession 被 drop 时，其内部的 mpsc::Sender 被 drop。
            }
        }
    };
    
    // 构建监听地址
    let listen_addr = format!("{}:{}", ws_config.host, ws_config.port);

    // 启动实际的 WebSocket 服务
    if let Err(e) = start_server(listen_addr, on_new_connection_cb).await {
        error!("[WebSocket服务] WebSocket服务启动失败或遇到致命错误: {}", e);
        // 旧: return Err::<(), _>(e.into()).context("WebSocket服务运行失败");
        return Err(anyhow::Error::from(e)).context("WebSocket服务运行失败");
    }
    
    warn!("[WebSocket服务] WebSocket服务意外停止。start_server 函数已返回。");
    Ok(())
} 