// SatOnSiteMobile/src-tauri/src/commands/task_cmds.rs

//! `SatOnSiteMobile` (现场端移动应用) 的任务相关 Tauri 命令模块。
//!
//! 本模块旨在包含所有与任务管理、任务流程控制等相关的 Tauri 命令。
//! 例如，如果现场端需要通过命令（而非完全通过 WebSocket 消息流）查询任务列表、
//! 选择或激活特定任务、报告任务级状态等，则相关命令应在此处定义。
//!
//! **当前状态**: 此模块为占位符，等待根据项目开发计划（例如 P7.4.x）的具体需求填充命令。
//!
//! ## 未来可能的命令示例:
//! ```rust
//! // #[tauri::command]
//! // async fn get_available_tasks(app_handle: tauri::AppHandle) -> Result<Vec<String>, String> {
//! //     // 实现获取可用任务列表的逻辑...
//! //     Ok(vec!["任务A".to_string(), "任务B".to_string()])
//! // }
//! 
//! // #[tauri::command]
//! // async fn set_active_task(app_handle: tauri::AppHandle, task_id: String) -> Result<(), String> {
//! //     // 实现设置当前活动任务的逻辑...
//! //     log::info!("[现场端任务命令] 设置活动任务ID: {}", task_id);
//! //     Ok(())
//! // }
//! ```

// 初始占位：此模块当前为空。
// 后续将根据项目开发计划（例如项目阶段 P7.4.x 中定义的需求）
// 在此处添加与任务操作相关的具体 Tauri 命令函数。
// 请确保所有新增命令都遵循标准的错误处理和日志记录规范。 

use tauri::State;
use std::sync::Arc;
use log::{info, error};
use serde_json;

use crate::ws_client::service::WebSocketClientService;
use common_models::ws_payloads::{UpdateTaskDebugNotePayload, UPDATE_TASK_DEBUG_NOTE_MESSAGE_TYPE, GeneralResponse};
use rust_websocket_utils::message::WsMessage;

#[tauri::command]
pub async fn update_task_debug_note_cmd(
    group_id: String, 
    new_note: String, 
    custom_shared_data_json_string: Option<String>, 
    ws_client_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<GeneralResponse, String> {
    info!(
        "[中心端CMD::update_task_debug_note] GroupID: '{}', Note: '{}', CustomData: {:?}",
        group_id, new_note, custom_shared_data_json_string
    );

    if !ws_client_service.is_connected().await {
        let err_msg = "无法更新调试备注：WebSocket未连接。".to_string();
        error!("[中心端CMD] {}", err_msg);
        return Err(err_msg);
    }

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
                    Ok(GeneralResponse {
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