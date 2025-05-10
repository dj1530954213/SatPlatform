// SatCloudService/src-tauri/src/ws_server/message_router.rs

//! 负责处理从客户端接收到的 WebSocket 消息，并根据消息类型进行分发处理。

use std::sync::Arc;
use anyhow::Result; // 使用 anyhow 作为错误类型，简化错误处理
use log::{debug, warn, error}; // 使用 log crate 进行日志记录
use chrono::Utc; // 用于获取当前时间

use common_models::ws_payloads::{
    self, // 引入整个模块，方便访问其下的常量和结构体
    EchoPayload,
    ErrorResponsePayload,
    PingPayload, // P1.4.1 新增
    PongPayload, // P1.4.1 新增
};
use rust_websocket_utils::message::WsMessage; // WsMessage 结构体
use super::client_session::ClientSession; // ClientSession 结构体

/// 异步处理从客户端接收到的单个 WebSocket 消息。
///
/// 此函数由 `rust_websocket_utils` 的 `TransportLayer` 在接收到消息后调用。
///
/// # 参数
/// * `client_session`: 发送此消息的客户端会话的共享引用。
/// * `message`: 从客户端接收到的 `WsMessage`。
///
/// # 返回
/// * `Result<(), anyhow::Error>`: 如果处理成功则返回 `Ok(())`，否则返回包含错误的 `Err`。
///   注意：即使向特定客户端发送响应失败，此函数也可能返回 `Ok(())`，
///   因为这通常不应导致整个服务或消息处理循环终止。此类错误会被记录。
pub async fn handle_message(
    client_session: Arc<ClientSession>,
    message: WsMessage,
) -> Result<(), anyhow::Error> {
    // 1. 更新客户端的 last_seen 时间戳
    let now = Utc::now();
    *client_session.last_seen.write().await = now;
    // --- P1.4.1_Test (场景2) 日志增强 ---
    debug!(
        "客户端 {}: last_seen 已更新为: {}",
        client_session.client_id, now.to_rfc3339()
    );
    // --- 结束日志增强 ---

    debug!(
        "客户端 {}: 收到消息，类型: '{}'，原始负载: '{}'",
        client_session.client_id, message.message_type, message.payload
    );

    // 2. 根据消息类型进行分发处理
    match message.message_type.as_str() {
        ws_payloads::ECHO_MESSAGE_TYPE => {
            // 处理 Echo 消息
            match serde_json::from_str::<EchoPayload>(&message.payload) {
                Ok(echo_payload) => {
                    debug!(
                        "客户端 {}: EchoPayload 解析成功: content='{}'",
                        client_session.client_id, echo_payload.content
                    );

                    // 准备回显消息
                    match WsMessage::new(ws_payloads::ECHO_MESSAGE_TYPE.to_string(), &echo_payload) {
                        Ok(response_msg) => {
                            // 通过 client_session 的 sender 将响应异步发送回该客户端
                            if let Err(e) = client_session.sender.send(response_msg).await {
                                error!(
                                    "客户端 {}: 发送 Echo 响应失败: {}",
                                    client_session.client_id, e
                                );
                            } else {
                                debug!(
                                    "客户端 {}: Echo 响应已成功发送。",
                                    client_session.client_id
                                );
                            }
                        }
                        Err(e) => {
                            error!(
                                "客户端 {}: 创建 Echo 响应 WsMessage 失败: {}",
                                client_session.client_id, e
                            );
                        }
                    }
                }
                Err(e) => {
                    // EchoPayload 反序列化失败
                    warn!(
                        "客户端 {}: 解析 EchoPayload 失败: {}。原始 payload: '{}'",
                        client_session.client_id, e, message.payload
                    );
                    // (推荐) 向客户端发送 ErrorResponse
                    send_error_response(
                        &client_session,
                        Some(ws_payloads::ECHO_MESSAGE_TYPE.to_string()),
                        format!("无效的 EchoPayload: {}", e),
                    )
                    .await;
                }
            }
        }
        // --- P1.4.1 新增 Ping 处理 ---
        ws_payloads::PING_MESSAGE_TYPE => {
            // 客户端发送 Ping 消息，服务端响应 Pong
            // (可选) 尝试解析 PingPayload，如果它未来可能包含数据
            match serde_json::from_str::<PingPayload>(&message.payload) {
                Ok(_ping_payload) => { // _ping_payload 目前为空，所以忽略
                    debug!("客户端 {}: 收到 Ping 消息。", client_session.client_id);
                    
                    // 准备 Pong 响应
                    let pong_payload = PongPayload {}; // PongPayload 当前也为空
                    match WsMessage::new(ws_payloads::PONG_MESSAGE_TYPE.to_string(), &pong_payload) {
                        Ok(pong_msg) => {
                            if let Err(e) = client_session.sender.send(pong_msg).await {
                                error!(
                                    "客户端 {}: 发送 Pong 响应失败: {}",
                                    client_session.client_id, e
                                );
                            } else {
                                debug!(
                                    "客户端 {}: Pong 响应已成功发送。",
                                    client_session.client_id
                                );
                            }
                        }
                        Err(e) => {
                            error!(
                                "客户端 {}: 创建 Pong 响应 WsMessage 失败: {}",
                                client_session.client_id, e
                            );
                        }
                    }
                }
                Err(e) => {
                    // PingPayload 反序列化失败 (虽然当前为空，但为未来扩展保留)
                    warn!(
                        "客户端 {}: 解析 PingPayload 失败: {}。原始 payload: '{}'",
                        client_session.client_id, e, message.payload
                    );
                    // 通常，对于 Ping 失败，不一定会发送 ErrorResponse，因为这可能导致不必要的流量
                    // 且 Ping 的 payload 预期为空。如果 Ping 消息的 payload 格式错误，可以仅记录日志。
                    // 如果需要，也可以取消下面注释以发送错误：
                    // send_error_response(
                    // &client_session,
                    // Some(ws_payloads::PING_MESSAGE_TYPE.to_string()),
                    // format!("无效的 PingPayload: {}", e),
                    // )
                    // .await;
                }
            }
        }
        // --- P1.4.1 结束 ---
        _ => {
            // 处理未知消息类型
            warn!(
                "客户端 {}: 收到未知消息类型: '{}'",
                client_session.client_id, message.message_type
            );
            // (推荐) 向客户端发送 ErrorResponse，告知消息类型不被支持
            send_error_response(
                &client_session,
                Some(message.message_type.clone()), // 原始消息类型
                format!("不支持的消息类型: '{}'", message.message_type),
            )
            .await;
        }
    }

    Ok(()) // 函数正常处理完毕
}

/// 辅助函数：向客户端发送标准错误响应。
///
/// # 参数
/// * `client_session`: 目标客户端会话。
/// * `original_message_type`: 可选，原始请求的消息类型。
/// * `error_message`: 错误的描述文本。
async fn send_error_response(
    client_session: &Arc<ClientSession>,
    original_message_type: Option<String>,
    error_message: String,
) {
    let error_payload = ErrorResponsePayload {
        original_message_type,
        error: error_message.clone(), // Clone error_message for logging
    };

    match WsMessage::new(ws_payloads::ERROR_RESPONSE_MESSAGE_TYPE.to_string(), &error_payload) {
        Ok(error_msg) => {
            if let Err(e) = client_session.sender.send(error_msg).await {
                error!(
                    "客户端 {}: 发送 ErrorResponse 失败: {}",
                    client_session.client_id, e
                );
            } else {
                debug!(
                    "客户端 {}: ErrorResponse 已发送: {}",
                    client_session.client_id, error_message
                );
            }
        }
        Err(e) => {
            error!(
                "客户端 {}: 创建 ErrorResponse WsMessage 失败: {}. 原始错误信息: '{}'",
                client_session.client_id, e, error_message
            );
        }
    }
} 