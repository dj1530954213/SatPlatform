use tauri::State;
use std::sync::Arc;
use log::{info, error};
use serde_json; // 添加 serde_json

// 使用现场端修正后的路径
use crate::ws_client::service::WebSocketClientService; 
use common_models::ws_payloads::{UpdateTaskDebugNotePayload, UPDATE_TASK_DEBUG_NOTE_MESSAGE_TYPE};
use rust_websocket_utils::message::WsMessage;

// 使用现场端的 GenericResponse 定义
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct GenericResponse {
    pub success: bool,
    pub message: String,
}

// 命令重命名或调整参数以匹配现场端。我们这里采用现场端的参数结构。
// 可以考虑重命名为 send_debug_note_from_center_cmd 以区分，或保持原名但内部逻辑对齐。
// 这里我们修改现有命令以匹配功能。
#[tauri::command]
pub async fn update_task_debug_note_cmd(
    group_id: String, // 新增参数
    new_note: String, // 新增参数
    custom_shared_data_json_string: Option<String>, // 新增参数
    ws_client_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<GenericResponse, String> {
    info!(
        "[中心端CMD::update_task_debug_note] GroupID: '{}', Note: '{}', CustomData: {:?}",
        group_id, new_note, custom_shared_data_json_string
    );

    if !ws_client_service.is_connected().await {
        let err_msg = "无法更新调试备注：WebSocket未连接。".to_string();
        error!("[中心端CMD] {}", err_msg);
        // 返回现场端风格的 GenericResponse 错误
        return Err(err_msg);
    }

    // 解析 custom_shared_data_json_string
    let custom_data_value: Option<serde_json::Value> = match custom_shared_data_json_string {
        Some(json_str) if !json_str.trim().is_empty() => {
            match serde_json::from_str(&json_str) {
                Ok(val) => Some(val),
                Err(e) => {
                    let err_msg = format!("[中心端CMD] 无效的JSON格式 for custom_shared_data: {}. 内容: '{}'", e, json_str);
                    error!("{}", err_msg);
                    return Err(err_msg);
                }
            }
        }
        _ => None,
    };

    // 构建 payload
    let payload = UpdateTaskDebugNotePayload {
        group_id: group_id.clone(),
        new_note,
        custom_shared_data: custom_data_value,
    };

    match WsMessage::new(
        UPDATE_TASK_DEBUG_NOTE_MESSAGE_TYPE.to_string(),
        &payload, 
    ) {
        Ok(ws_message) => {
            match ws_client_service.send_ws_message(ws_message).await {
                Ok(_) => {
                    let success_msg = format!("[中心端CMD] 调试备注消息已成功发送至服务器，组ID: '{}'.", group_id);
                    info!("{}", success_msg);
                    Ok(GenericResponse {
                        success: true,
                        message: success_msg,
                    })
                }
                Err(e) => {
                    let err_msg = format!("[中心端CMD] 发送调试备注消息至服务器失败，组ID: '{}': {:?}", group_id, e);
                    error!("{}", err_msg);
                    Err(err_msg)
                }
            }
        }
        Err(e) => {
            let err_msg = format!("[中心端CMD] 创建WsMessage (UpdateTaskDebugNote) 失败，组ID: '{}': {:?}", group_id, e);
            error!("{}", err_msg);
            Err(err_msg)
        }
    }
} 