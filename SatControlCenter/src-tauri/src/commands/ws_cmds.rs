// SatControlCenter/src-tauri/src/commands/ws_cmds.rs
// 占位符：WebSocket操作相关命令

use tauri::State;
use std::sync::Arc;
use log::{info, error};

use crate::ws_client::service::WebSocketClientService;
// 导入 GeneralResponse 并移除不再需要的 CommonGeneralResponse 别名
use common_models::ws_payloads::{RegisterPayload, EchoPayload, GeneralResponse};
use common_models::enums::ClientRole;

#[tauri::command]
pub async fn connect_to_ws_server_cmd(
    ws_client_service: State<'_, Arc<WebSocketClientService>>,
    url: String,
) -> Result<GeneralResponse, String> {
    info!("[中心端CMD] connect_to_ws_server_cmd called with URL: {}", url);
    match ws_client_service.connect(&url).await {
        Ok(_) => Ok(GeneralResponse { success: true, message: "Successfully connected to WebSocket server.".to_string() }),
        Err(e) => {
            error!("[中心端CMD] Failed to connect to WebSocket server: {:?}", e);
            Err(format!("Failed to connect: {:?}", e))
        }
    }
}

#[tauri::command]
pub async fn disconnect_from_ws_server_cmd(
    ws_client_service: State<'_, Arc<WebSocketClientService>>,
) -> Result<GeneralResponse, String> {
    info!("[中心端CMD] disconnect_from_ws_server_cmd called.");
    match ws_client_service.disconnect().await {
        Ok(_) => Ok(GeneralResponse { success: true, message: "Successfully disconnected from WebSocket server.".to_string() }),
        Err(e) => {
            error!("[中心端CMD] Failed to disconnect from WebSocket server: {:?}", e);
            Err(format!("Failed to disconnect: {:?}", e))
        }
    }
}

#[tauri::command]
pub async fn send_echo_message_cmd(
    ws_client_service: State<'_, Arc<WebSocketClientService>>,
    content: String,
) -> Result<GeneralResponse, String> {
    info!("[中心端CMD] send_echo_message_cmd called with content: {}", content);
    let echo_payload = EchoPayload { content };
    match ws_client_service.send_echo_message(echo_payload).await {
        Ok(_) => Ok(GeneralResponse { success: true, message: "Echo message sent.".to_string() }),
        Err(e) => Err(format!("Failed to send echo message: {:?}", e)),
    }
}

#[tauri::command]
pub async fn send_register_message_cmd(
    ws_client_service: State<'_, Arc<WebSocketClientService>>,
    group_id: String,
    task_id: String,
    // display_name: Option<String>, // 此命令暂不处理 display_name，由 general_cmds 中的 register_client_with_task 处理
    // client_software_version: Option<String>, // 此命令暂不处理 client_software_version
) -> Result<GeneralResponse, String> {
    info!("[中心端CMD] send_register_message_cmd for group: {}, task: {}", group_id, task_id);
    let register_payload = RegisterPayload {
        group_id,
        task_id,
        role: ClientRole::ControlCenter, 
        client_software_version: Some(env!("CARGO_PKG_VERSION").to_string()), // 与 general_cmds 保持一致
        client_display_name: Some("ControlCenterViaWsCmds".to_string()), // 提供一个默认的或考虑是否需要从参数传入
    };

    match ws_client_service.send_specific_message(common_models::ws_payloads::REGISTER_MESSAGE_TYPE, &register_payload).await {
        Ok(_) => Ok(GeneralResponse { success: true, message: "Register message sent.".to_string() }),
        Err(e) => Err(format!("Failed to send register message: {:?}", e)),
    }
}

// P4.2.1: 确保 GeneralResponse 结构体在 common_models 或这里定义
// 如果 common_models::ws_payloads::GeneralResponse 存在且适用，优先使用它
// 否则，如果需要一个与现场端 task_commands.rs 中类似的本地定义，则如下：
// #[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
// pub struct GeneralResponse { // 与 GenericResponse 重名，需要调整
//     pub success: bool,
//     pub message: String,
// }
// 使用上面已有的 GeneralResponse 即可。 