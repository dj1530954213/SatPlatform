use tauri::State;
use std::sync::Arc;
use log::{info, error};

// 调整以下 use 语句的路径以匹配您的项目结构
use crate::services::ws_client::service::WebSocketClientService; 
use common_models::ws_payloads::{UpdateTaskDebugNotePayload, UPDATE_TASK_DEBUG_NOTE_MESSAGE_TYPE};
use rust_websocket_utils::message::WsMessage; // 确保 WsMessage 在此作用域

/// 一个通用的响应结构体，用于简单的命令成功/失败反馈。
#[derive(Clone, serde::Serialize)]
struct GenericResponse {
    success: bool,
    message: Option<String>,
}

#[tauri::command]
pub async fn update_task_debug_note_cmd(
    payload: UpdateTaskDebugNotePayload, // 前端直接传递这个结构体
    ws_client_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<GenericResponse, String> {
    info!(
        "[中心端CMD] update_task_debug_note_cmd called for group_id: '{}', new_note: '{}'",
        payload.group_id, payload.new_note
    );

    if !ws_client_service.is_connected().await {
        let err_msg = "无法更新调试备注：WebSocket未连接。".to_string();
        error!("[中心端CMD] {}", err_msg);
        return Err(err_msg);
    }

    // 构建 WsMessage
    // 假设 WsMessage::new_with_payload 能够处理 Option<serde_json::Value>
    // 或者您有一个可以接受序列化后 serde_json::Value 的构造函数
    match serde_json::to_value(&payload) {
        Ok(json_payload_value) => {
            // 注意：rust_websocket_utils::message::WsMessage::new 可能需要具体的 Payload 类型
            // 而不是 serde_json::Value。如果 WsMessage::new 需要具体的泛型参数，
            // 我们可能需要调整 WsMessage 的创建方式或其定义。
            // 这里的实现假设 WsMessage::new 或类似的构造函数可以接受一个 serde_json::Value
            // 或者您有一个辅助函数来创建它。

            // 使用 WsMessage::new (假设它能处理)
            // 如果 WsMessage::new 需要 Payload: T, 我们需要将 json_payload_value 包裹或转换
            // 这里我们直接使用它，如果编译失败，说明 WsMessage::new 的签名不匹配。
            match WsMessage::new(
                UPDATE_TASK_DEBUG_NOTE_MESSAGE_TYPE.to_string(), 
                &json_payload_value // 传递引用，如果 WsMessage::new 接受 &T
                                    // 或者传递值 json_payload_value 如果 WsMessage::new 接受 T
            ) {
                Ok(ws_message) => {
                    if let Err(e) = ws_client_service.send_ws_message(ws_message).await {
                        error!("[中心端CMD] 发送 UpdateTaskDebugNoteCommand 失败: {}", e);
                        Err(format!("发送更新备注命令失败: {}", e))
                    } else {
                        info!("[中心端CMD] UpdateTaskDebugNoteCommand 已成功发送。");
                        Ok(GenericResponse { success: true, message: Some("更新备注请求已发送".to_string()) })
                    }
                }
                Err(e) => {
                     let err_msg = format!("[中心端CMD] 创建 WsMessage (UpdateTaskDebugNoteCommand) 失败: {}", e);
                     error!("{}", err_msg);
                     Err(err_msg)
                }
            }
        }
        Err(e) => {
            let err_msg = format!("[中心端CMD] 序列化 UpdateTaskDebugNotePayload 失败: {}", e);
            error!("{}", err_msg);
            Err(err_msg)
        }
    }
} 