use tauri::State;
use std::sync::Arc;
use log::{info, error};
// use uuid::Uuid; // WsMessage::new 通常会处理 message_id
// use chrono::Utc; // WsMessage::new 通常会处理 timestamp

use common_models::ws_payloads::{UpdateTaskDebugNotePayload, UPDATE_TASK_DEBUG_NOTE_MESSAGE_TYPE};
use rust_websocket_utils::message::WsMessage; // 假设路径和定义
use crate::services::ws_client_service::WebSocketClientService; // 假设路径

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct GenericResponse {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub async fn send_debug_note_from_site_cmd(
    group_id: String,
    new_note: String,
    ws_client_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<GenericResponse, String> {
    info!(
        "[SiteCMD::send_debug_note] Attempting to send debug note. GroupID: '{}', Note: '{}'",
        group_id, new_note
    );

    if !ws_client_service.is_connected().await {
        let err_msg = "WebSocket client is not connected. Cannot send debug note.";
        error!("[SiteCMD::send_debug_note] {}", err_msg);
        return Err(err_msg.to_string());
    }

    let payload = UpdateTaskDebugNotePayload {
        group_id: group_id.clone(),
        new_note,
    };

    match WsMessage::new(
        UPDATE_TASK_DEBUG_NOTE_MESSAGE_TYPE.to_string(),
        &payload, // WsMessage::new 应该能处理序列化
    ) {
        Ok(ws_message) => {
            match ws_client_service.send_ws_message_to_server(ws_message).await {
                Ok(_) => {
                    let success_msg = format!("Debug note message sent successfully to server for group '{}'.", group_id);
                    info!("[SiteCMD::send_debug_note] {}", success_msg);
                    Ok(GenericResponse {
                        success: true,
                        message: success_msg,
                    })
                }
                Err(e) => {
                    let err_msg = format!("Failed to send debug note message to server for group '{}': {:?}", group_id, e);
                    error!("[SiteCMD::send_debug_note] {}", err_msg);
                    Err(err_msg)
                }
            }
        }
        Err(e) => {
            let err_msg = format!("Failed to create WsMessage for debug note for group '{}': {:?}", group_id, e);
            error!("[SiteCMD::send_debug_note] {}", err_msg);
            Err(err_msg)
        }
    }
}

// 如果此文件是新建的，您可能还需要一个 mod.rs 在 commands 目录下:
/*
// SatOnSiteMobile/src-tauri/src/commands/mod.rs
pub mod task_commands;
// pub mod other_command_modules;

pub use task_commands::*;
// pub use other_command_modules::*;
*/ 