// SatOnSiteMobile/src-tauri/src/commands/ws_cmds.rs
// Placeholder for WebSocket Related Commands 
use tauri::State;
use std::sync::Arc;
use crate::ws_client::service::WebSocketClientService; // 假设 ws_client_service 在此路径
use common_models::ws_payloads::GeneralResponse; // 假设一个通用响应结构

#[tauri::command]
pub async fn connect_to_ws_server_cmd(
    server_url: String,
    ws_client_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<GeneralResponse, String> {
    log::info!("[WsCMD::connect_to_ws_server] Attempting to connect to WebSocket server: {}", server_url);
    match ws_client_service.connect(&server_url).await {
        Ok(_) => Ok(GeneralResponse { success: true, message: "Successfully connected to WebSocket server.".to_string() }),
        Err(e) => Err(format!("Failed to connect to WebSocket server: {}", e)),
    }
}

#[tauri::command]
pub async fn disconnect_from_ws_server_cmd(
    ws_client_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<GeneralResponse, String> {
    log::info!("[WsCMD::disconnect_from_ws_server] Attempting to disconnect from WebSocket server...");
    match ws_client_service.disconnect().await {
        Ok(_) => Ok(GeneralResponse { success: true, message: "Successfully disconnected from WebSocket server.".to_string() }),
        Err(e) => Err(format!("Failed to disconnect from WebSocket server: {}", e)),
    }
}

#[tauri::command]
pub async fn send_echo_message_cmd(
    message: String,
    _ws_client_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<GeneralResponse, String> {
    log::info!("[WsCMD::send_echo_message] Attempting to send echo message: {}", message);
    // 实际会构建 WsMessage 并发送
    // ws_client_service.send_ws_message(...).await;
    Ok(GeneralResponse { success: true, message: format!("Echo message sent (mock): {}", message) })
}

// 对应 P4.1.1 中定义的 register_to_debug_group_cmd，但错误日志中为 send_register_message_cmd
#[tauri::command]
pub async fn send_register_message_cmd(
    group_id: String,
    task_id: String,
    display_name: String,
    _ws_client_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<GeneralResponse, String> {
    log::info!(
        "[WsCMD::send_register_message] Attempting to register. GroupID: '{}', TaskID: '{}', Name: '{}'",
        group_id, task_id, display_name
    );
    // 实际的注册逻辑会构建 RegisterPayload 和 WsMessage，然后发送
    // let payload = common_models::ws_payloads::RegisterPayload { ... };
    // let ws_message = common_models::message::WsMessage::new(...);
    // match ws_client_service.send_ws_message(ws_message).await { ... }
    Ok(GeneralResponse { success: true, message: "Registration message sent (mock).".to_string() })
} 