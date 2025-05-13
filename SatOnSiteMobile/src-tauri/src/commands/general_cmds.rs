// SatOnSiteMobile/src-tauri/src/commands/general_cmds.rs

//! `SatOnSiteMobile` (现场端移动应用) 的通用 Tauri 命令模块。
//!
//! 本模块定义了可由前端 UI 通过 `invoke` API 调用的核心 Rust 函数。
//! 这些命令主要负责管理与云端服务的 WebSocket 连接（建立、断开、状态检查）、
//! 发送基础的测试消息（如 Echo），以及执行客户端向云端的注册流程。
//! 所有命令都应遵循清晰的错误处理模式，向前端返回 `Result<T, String>`，
//! 其中 `String` 包含用户可理解或可记录的错误信息。

use tauri::State; // 用于从 Tauri 状态管理器中注入共享状态
use log::{info, error}; // 日志记录宏 (移除了未使用的 warn)
use crate::ws_client::service::{WebSocketClientService}; // WebSocket 客户端服务
use crate::config::WsClientConfig; // 应用配置（例如，默认的 WebSocket URL）
use std::sync::Arc; // 原子引用计数，用于安全地共享服务实例
use tauri::AppHandle; // Tauri 应用句柄，可用于访问状态、发射事件等
use rust_websocket_utils::message::WsMessage; // WebSocket 消息的通用结构
use common_models; // 项目共享的数据模型和常量
// use chrono::Utc; // 移除了未使用的导入
// use uuid::Uuid; // 移除了未使用的导入
use common_models::ws_payloads::{RegisterPayload, REGISTER_MESSAGE_TYPE, EchoPayload, ECHO_MESSAGE_TYPE}; // WebSocket 消息负载定义和类型常量
use common_models::enums::ClientRole; // 客户端角色枚举
use tauri::Manager; // 引入 Manager trait 以便在 AppHandle 上使用 state() 等方法

/// [Tauri 命令] 连接到云端 WebSocket 服务器。
///
/// 当前端用户触发连接操作时，此命令被调用。它会尝试与指定的 URL 或配置文件中定义的默认 URL 建立 WebSocket 连接。
///
/// # 主要流程：
/// 1. 记录命令调用和传入的 URL 参数。
/// 2. 确定最终要连接的 URL：
///    - 如果 `url` 参数被提供，则使用该 URL。
///    - 否则，尝试从 `WsClientConfig::load()` 加载配置，并使用其中的 `cloud_ws_url`。
///    - 如果加载配置失败，则返回错误。
/// 3. 调用 `WebSocketClientService::connect()` 方法启动连接过程。
///
/// # 参数
/// * `_app_handle`: Tauri 应用句柄。虽然在此命令中当前未使用，但保留它是 Tauri 命令签名的标准做法，以备将来可能需要（例如发射事件）。
/// * `state`: `WebSocketClientService` 的共享状态实例，通过 Tauri 的依赖注入机制提供。
/// * `url`: 可选参数，指定要连接的 WebSocket 服务器的 URL 字符串。如果为 `None`，则会尝试使用配置文件中的默认设置。
///
/// # 返回
/// * `Result<(), String>`: 
///     - `Ok(())`: 表示连接过程已成功启动（注意：这并不意味着连接已完全建立，实际的连接成功或失败状态将通过 `WsConnectionStatusEvent` 事件异步通知前端）。
///     - `Err(String)`: 如果在启动连接过程中发生错误（例如，无法加载配置，URL 无效等），则返回包含中文错误描述的字符串。
#[tauri::command(rename_all = "snake_case")] // 命令名在前端将是 connect_to_cloud
pub async fn connect_to_cloud(
    _app_handle: AppHandle, // _app_handle 在此命令中未使用，故加下划线以消除警告
    state: State<'_, Arc<WebSocketClientService>>, // 从 Tauri 状态获取 WebSocketClientService
    url: Option<String>, // 可选的 WebSocket URL
) -> Result<(), String> {
    info!("[现场端通用命令] 'connect_to_cloud' 被调用，URL 参数: {:?}", url);

    let ws_service = state.inner().clone(); // 获取内部服务的 Arc 引用并克隆

    // 确定连接 URL
    let connect_url = match url {
        Some(u) => {
            info!("[现场端通用命令] 使用命令参数中提供的 URL: {}", u);
            u
        }
        None => {
            info!("[现场端通用命令] URL 未在命令参数中提供，尝试从配置文件加载默认 URL...");
            // 注意: WsClientConfig::load() 是 SatOnSiteMobile 特有的配置加载逻辑。
            match WsClientConfig::load() { // 尝试加载配置
                 Ok(client_config) => {
                    info!("[现场端通用命令] 从配置文件加载到 URL: {}", client_config.cloud_ws_url);
                    client_config.cloud_ws_url
                 }
                 Err(e) => {
                    let err_msg = format!("[现场端通用命令] 加载客户端配置 (WsClientConfig) 失败: {}. 请在调用时提供明确的 URL，或确保配置文件 'config/onsite_client_config.json' 存在且有效。", e);
                    error!("{}", err_msg);
                    return Err(err_msg); // 返回中文错误信息
                 }
            }
        }
    };

    info!("[现场端通用命令] 最终确定连接目标 URL: {}", connect_url);

    // 调用服务层执行连接操作
    match ws_service.connect(&connect_url).await {
        Ok(_) => {
            info!("[现场端通用命令] 'connect_to_cloud': WebSocket 连接过程已成功启动。后续状态将通过事件通知。");
            Ok(())
        }
        Err(e) => {
            let err_msg = format!("启动到云服务的连接过程失败: {}", e);
            error!("[现场端通用命令] 'connect_to_cloud': {}", err_msg);
            Err(err_msg) // 返回包含技术细节的中文错误信息
        }
    }
}

/// [Tauri 命令] 从云端 WebSocket 服务器断开连接。
///
/// 当前端用户请求断开连接时，此命令被调用。
///
/// # 参数
/// * `state`: `WebSocketClientService` 的共享状态实例。
///
/// # 返回
/// * `Result<(), String>`: 
///     - `Ok(())`: 表示断开连接请求已成功处理。
///     - `Err(String)`: 如果处理断开请求时发生错误，则返回包含中文错误描述的字符串。
#[tauri::command(rename_all = "snake_case")]
pub async fn disconnect_from_cloud(
    state: State<'_, Arc<WebSocketClientService>>,
) -> Result<(), String> {
    info!("[现场端通用命令] 'disconnect_from_cloud' 被调用。");
    let ws_service = state.inner().clone(); // 获取服务实例
    match ws_service.disconnect().await { // 调用服务层的断开方法
        Ok(_) => {
            info!("[现场端通用命令] 'disconnect_from_cloud': 断开连接请求已成功处理。");
            Ok(())
        }
        Err(e) => {
            let err_msg = format!("处理断开连接请求失败: {}", e);
            error!("[现场端通用命令] 'disconnect_from_cloud': {}", err_msg);
            Err(err_msg)
        }
    }
}

/// [Tauri 命令] 检查当前 WebSocket 的连接状态。
///
/// 允许前端查询后端维护的 WebSocket 连接是否处于活动状态。
///
/// # 参数
/// * `state`: `WebSocketClientService` 的共享状态实例。
///
/// # 返回
/// * `Result<bool, String>`: 
///     - `Ok(true)`: 如果当前已连接。
///     - `Ok(false)`: 如果当前未连接。
///   此命令通常不应失败，但为保持 Tauri 命令返回 `Result` 的一致性，仍使用此签名。
#[tauri::command(rename_all = "snake_case")]
pub async fn check_ws_connection_status(
    state: State<'_, Arc<WebSocketClientService>>,
) -> Result<bool, String> {
    info!("[现场端通用命令] 'check_ws_connection_status' 被调用。");
    let ws_service = state.inner().clone();
    let is_connected = ws_service.is_connected().await; // 调用服务层检查状态
    info!("[现场端通用命令] 当前 WebSocket 连接状态: {}", if is_connected { "已连接" } else { "未连接" });
    Ok(is_connected)
}

/// [Tauri 命令] 发送 Echo (回声) 消息到云端 WebSocket 服务器。
///
/// 主要用于测试 WebSocket 连接的连通性以及消息双向传输是否正常。
///
/// # 主要流程：
/// 1. 构建 `EchoPayload`。
/// 2. 将 `EchoPayload` 序列化为 JSON 字符串。
/// 3. 构建包含此 JSON payload 的 `WsMessage`。
/// 4. 通过 `WebSocketClientService::send_ws_message()` 发送消息。
///
/// # 参数
/// * `state`: `WebSocketClientService` 的共享状态实例。
/// * `content`: 要包含在 Echo 消息中的字符串内容。
///
/// # 返回
/// * `Result<(), String>`: 
///     - `Ok(())`: 如果 Echo 消息已成功排队等待发送。
///     - `Err(String)`: 如果在构建或发送消息过程中发生错误，则返回包含中文错误描述的字符串。
#[tauri::command(rename_all = "snake_case")]
pub async fn send_ws_echo(
    state: State<'_, Arc<WebSocketClientService>>,
    content: String,
) -> Result<(), String> {
    info!("[现场端通用命令] 'send_ws_echo' 被调用, 发送内容: {:?}", content);
    let ws_service = state.inner().clone();

    // 1. 构建 EchoPayload
    // common_models::ws_payloads::EchoPayload 应该已经在 common_models 中定义
    let echo_payload = EchoPayload { content: content.clone() }; // 克隆 content 用于 payload

    // 2. 序列化 EchoPayload 为 JSON 字符串
    // 此步骤已由 WsMessage::new 内部处理，故省略显式序列化

    // 3. 构建 WsMessage
    // 使用 WsMessage::new 辅助函数创建消息，更简洁且减少手动错误
    // 注意：WsMessage::new 期望 payload 是一个实现了 Serialize 的结构体引用
    let ws_message = match WsMessage::new(ECHO_MESSAGE_TYPE.to_string(), &echo_payload) {
        Ok(msg) => msg,
        Err(e) => {
            let err_msg = format!("创建 Echo 类型的 WsMessage 失败: {}", e);
            error!("[现场端通用命令] {}", err_msg);
            return Err(err_msg);
        }
    };
    
    // 旧的 WsMessage 构建方式，已被 WsMessage::new 替代
    // let payload_json = match serde_json::to_string(&echo_payload) {
    //     Ok(json) => json,
    //     Err(e) => {
    //         let err_msg = format!("序列化 EchoPayload 失败: {}", e);
    //         error!("[现场端通用命令] {}", err_msg);
    //         return Err(err_msg);
    //     }
    // };
    // let ws_message = WsMessage {
    //     message_id: Uuid::new_v4().to_string(),
    //     timestamp: Utc::now().timestamp_millis(),
    //     message_type: ECHO_MESSAGE_TYPE.to_string(),
    //     payload: payload_json,
    // };

    // 4. 发送消息
    match ws_service.send_ws_message(ws_message).await {
        Ok(_) => {
            info!("[现场端通用命令] Echo 消息 (内容: {:?}) 已成功通过 WebSocket 服务发送。", content);
            Ok(())
        }
        Err(e) => {
            let err_msg = format!("发送 Echo 消息失败: {}", e);
            error!("[现场端通用命令] {}", err_msg);
            Err(err_msg)
        }
    }
}

/// [Tauri 命令] 向云端 WebSocket 服务器注册客户端并关联到指定的任务组。
///
/// 此命令是客户端（现场端）加入调试会话的关键步骤。它向云端表明自己的身份（角色）、
/// 希望加入的任务组 ID，以及当前操作关联的具体任务 ID。
///
/// # 主要流程：
/// 1. 构建 `RegisterPayload`，包含 `group_id`, `role` (固定为 `ClientRole::OnSiteMobile`), 和 `task_id`。
/// 2. 将 `RegisterPayload` 序列化为 JSON 字符串。
/// 3. 构建包含此 JSON payload 的 `WsMessage` (类型为 `REGISTER_MESSAGE_TYPE`)。
/// 4. 从 `AppHandle` 获取 `WebSocketClientService` 的状态实例。
/// 5. 通过 `WebSocketClientService::send_ws_message()` 发送注册消息。
///
/// # 参数
/// * `app_handle`: Tauri 应用句柄，用于获取共享的 `WebSocketClientService` 状态。
/// * `group_id`: 字符串，客户端希望加入或创建的调试组的唯一标识符。
/// * `task_id`: 字符串，与此调试会话关联的具体任务的唯一标识符。
///
/// # 返回
/// * `Result<(), String>`: 
///     - `Ok(())`: 如果注册消息已成功排队等待发送。实际的注册成功与否将通过后续的 "RegisterResponse" (`WsRegistrationStatusEvent`) 事件进行通知。
///     - `Err(String)`: 如果在构建或发送注册消息过程中发生错误，则返回包含中文错误描述的字符串。
#[tauri::command(rename_all = "snake_case")]
pub async fn register_client_with_task(
    app_handle: tauri::AppHandle, // 使用 AppHandle 获取状态
    group_id: String,
    task_id: String,
) -> Result<(), String> {
    info!(
        "[现场端通用命令] 'register_client_with_task' 被调用, 组ID: {}, 任务ID: {}",
        group_id, task_id
    );

    // 1. 构建 RegisterPayload
    let register_payload = RegisterPayload {
        group_id: group_id.clone(), // 克隆以保持所有权
        role: ClientRole::OnSiteMobile, // 现场端角色
        task_id: task_id.clone(),   // 克隆以保持所有权
    };

    // 2. & 3. 构建 WsMessage (使用 WsMessage::new 辅助函数)
    let ws_message = match WsMessage::new(REGISTER_MESSAGE_TYPE.to_string(), &register_payload) {
        Ok(msg) => msg,
        Err(e) => {
            let err_msg = format!("创建注册请求类型的 WsMessage 失败: {}", e);
            error!("[现场端通用命令] {}", err_msg);
            return Err(err_msg);
        }
    };

    // 旧的序列化和 WsMessage 构建方式，已被 WsMessage::new 替代
    // let serialized_payload = match serde_json::to_string(&register_payload) {
    //     Ok(json_str) => json_str,
    //     Err(e) => {
    //         error!("[现场端通用命令] 序列化 RegisterPayload 失败: {}", e);
    //         return Err(format!("构建注册请求数据失败: {}", e));
    //     }
    // };
    // let ws_message = WsMessage {
    //     message_id: Uuid::new_v4().to_string(),
    //     timestamp: Utc::now().timestamp_millis(),
    //     message_type: REGISTER_MESSAGE_TYPE.to_string(),
    //     payload: serialized_payload,
    // };

    // 4. 获取 WsClientService 实例
    // 使用 app_handle.state() 是更推荐的获取 Tauri 托管状态的方式
    let ws_service = match app_handle.try_state::<Arc<WebSocketClientService>>() {
        Some(state_val) => state_val.inner().clone(), // 获取 Arc<Service> 并克隆 Arc
        None => {
            let err_msg = "[现场端通用命令] 无法从 Tauri AppHandle 获取 WebSocketClientService 状态。服务可能未正确初始化或注册。";
            error!("{}", err_msg);
            return Err("内部服务器错误: WebSocket 服务状态不可用".to_string());
        }
    };

    // 5. 发送消息
    match ws_service.send_ws_message(ws_message).await {
        Ok(_) => {
            info!(
                "[现场端通用命令] 已通过 WsService 发送 'Register' 消息, 组ID: {}, 任务ID: {}",
                group_id, task_id
            );
            Ok(())
        }
        Err(e) => {
            let err_msg = format!("通过 WsService 发送 'Register' 消息失败: {}", e);
            error!("[现场端通用命令] {}", err_msg);
            Err(err_msg)
        }
    }
}

// 提示：后续根据项目开发步骤 (例如 P4.2.1 - 客户端数据同步功能) 的需要，
// 可能会在此文件或新创建的、按功能划分的 `*_cmds.rs` 文件中添加更多业务相关的 Tauri 命令。
// 例如: `send_pre_check_item_update_command`, `send_single_test_step_feedback_command` 等。
// 这些命令将负责处理更具体的业务逻辑，如将前端用户的操作（完成预检、上报测试结果）
// 封装成相应的 WebSocket 消息发送到云端。 