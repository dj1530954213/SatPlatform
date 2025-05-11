// SatControlCenter/src-tauri/src/event.rs

//! `SatControlCenter` (卫星控制中心) 应用事件定义模块。
//!
//! 本模块集中定义了所有用于 Tauri 后端 (Rust) 与前端 (Angular) 之间进行异步通信的事件。
//! 这些事件主要通过 Tauri 的事件系统 (`emit` 和 `listen` API) 进行发送和接收。
//! 定义的内容包括：
//! - **事件名称常量**: 字符串常量，作为事件的唯一标识符，在前端监听和后端发送时使用。
//! - **事件负载结构体**: 当事件需要传递复杂数据时，会定义相应的 Rust 结构体。
//!   这些结构体通常会派生 `serde::Serialize` 以便能够被序列化为 JSON 并发送给前端，
//!   同时也会派生 `Clone` 和 `Debug` 以方便使用。

use serde::Serialize; // 引入 Serialize trait，用于将 Rust 结构体序列化为 JSON 等格式，以便在 Tauri 事件中传递给前端。

// --- 云端 WebSocket 连接状态相关事件名称常量 --- 

/// 事件名称常量：表示已成功连接到云端 WebSocket 服务。
/// 当 `ws_client::WebSocketClientService` 成功与 `SatCloudService` 建立 WebSocket 连接后，
/// 可能会触发此事件，通知前端连接已就绪。
pub const EVENT_CLOUD_WS_CONNECTED: &str = "cloud-ws-connected";

/// 事件名称常量：表示尝试连接到云端 WebSocket 服务失败。
/// 当 `ws_client::WebSocketClientService` 尝试连接 `SatCloudService` 但未能成功时
/// (例如，网络问题、服务端未运行、URL错误等)，可能会触发此事件，通知前端连接尝试已失败。
/// 通常会伴随一个包含错误信息的负载 (例如 `WsConnectionStatusEvent`)。
pub const EVENT_CLOUD_WS_CONNECT_FAILED: &str = "cloud-ws-connect-failed";

// 提示：后续可以根据具体业务需求，在此处扩展更多与云端 WebSocket 通信相关的特定事件，
// 例如：收到特定类型的业务消息、连接意外断开、认证成功/失败等。

// 占位注释：以下为早前构思的事件常量，当前可能已被更具体的事件 (如 WS_CONNECTION_STATUS_EVENT) 所覆盖或整合。
// 根据实际开发进展，可以考虑移除或重新评估其必要性。
// 例如： pub const EVENT_WS_CONNECTED: &str = "event-ws-connected"; 

// --- WebSocket 详细连接状态变更事件 --- 

/// `WsConnectionStatusEvent` (WebSocket 连接状态事件) 的负载结构体定义。
///
/// 当 `SatControlCenter` 与云端服务 (`SatCloudService`) 的 WebSocket 连接状态发生任何显著变化时
/// (例如：首次尝试连接、成功建立连接、连接意外断开、或主动断开连接后)，
/// 后端会构建此结构体的实例，并作为负载通过名为 `WS_CONNECTION_STATUS_EVENT` 的 Tauri 事件发送给前端。
/// 前端可以监听此事件，以实时更新UI界面上显示的连接状态，并根据情况执行相应的逻辑。
#[derive(Clone, Serialize, Debug)] // 派生 Clone, Serialize, Debug trait
                                 // - Clone: 允许创建此结构体实例的副本。
                                 // - Serialize: 允许将此结构体实例序列化为 JSON，以便作为 Tauri 事件的负载发送给前端。
                                 // - Debug: 允许使用 `{:?}` 格式化操作符打印此结构体实例，方便调试。
pub struct WsConnectionStatusEvent {
    /// 指示当前 WebSocket 是否已成功连接到云端服务。
    /// - `true`: 表示连接已建立且处于活动状态。
    /// - `false`: 表示连接当前未建立，或已断开。
    pub connected: bool,

    /// 当连接成功时，由云端服务 (`SatCloudService`) 分配给本控制中心客户端的唯一标识符 (`client_id`)。
    /// - `Some(String)`: 如果 `connected` 为 `true` 且云端已成功分配 `client_id`，则此字段包含该 ID 字符串。
    /// - `None`: 如果 `connected` 为 `false`，或者连接虽已建立但云端尚未完成 `client_id` 的分配与回传，则此字段为 `None`。
    pub client_id: Option<String>,

    /// 当连接失败或意外断开时，此字段可能包含描述错误原因的文本信息。
    /// - `Some(String)`: 如果 `connected` 为 `false` 是由于连接尝试失败、或已建立的连接因错误而中断，
    ///   则此字段可能包含相关的错误提示信息。
    /// - `None`: 如果 `connected` 为 `true`，或者 `connected` 为 `false` 但并非由明确的错误导致 (例如，用户主动断开连接且无异常)，
    ///   则此字段通常为 `None`。
    pub error_message: Option<String>,
}

/// `WsConnectionStatusEvent` (WebSocket 连接状态事件) 的标准事件名称常量。
///
/// 后端 (Rust) 在发送 `WsConnectionStatusEvent` (WebSocket 连接状态事件) 数据时，应使用此常量作为事件名称。
/// 前端 (Angular) 在监听此类事件时，也应使用此常量。
///
/// 前端 (TypeScript/Angular) 监听此事件的示例代码片段：
/// ```typescript
/// import {{ listen, Event }} from '@tauri-apps/api/event';
/// 
/// interface WsConnectionStatusPayload {{
///   connected: boolean;
///   client_id?: string;
///   error_message?: string;
/// }}
/// 
/// async function setupWsStatusListener() {{
///   await listen<WsConnectionStatusPayload>('{WS_CONNECTION_STATUS_EVENT}', (event: Event<WsConnectionStatusPayload>) => {{
///     console.log('WebSocket 连接状态发生变化:', event.payload);
///     if (event.payload.connected) {{
///       console.log('已连接到云端，客户端ID:', event.payload.client_id || '尚未分配');
///       // TODO: 更新 UI 显示为"已连接"，并处理 client_id
///     }} else {{
///       console.warn('已从云端断开或连接失败。错误信息:', event.payload.error_message || '无特定错误信息');
///       // TODO: 更新 UI 显示为"已断开"，并提示错误信息
///     }}
///   }});
///   console.log('已启动 WebSocket 连接状态事件监听器。');
/// }}
/// 
/// setupWsStatusListener();
/// ```
pub const WS_CONNECTION_STATUS_EVENT: &str = "ws_connection_status_v1"; // 版本化事件名，例如添加 _v1，便于未来升级

// --- Echo (回声测试) 响应事件 (项目阶段 P2.2.1 引入) --- 

/// `EchoResponseEventPayload` (Echo响应事件负载) 的标准事件名称常量。
///
/// 当 `SatControlCenter` 的后端通过 WebSocket 向云端服务发送了一个 Echo (回声) 请求，
/// 并且成功收到了云端的回声响应后，后端将使用此事件名称，
/// 将包含原始响应内容的 `EchoResponseEventPayload` (Echo响应事件负载) 发送给前端。
pub const ECHO_RESPONSE_EVENT: &str = "echo_response_event_v1"; // 版本化事件名

/// Echo (回声测试) 响应事件的 Payload (负载) 结构体定义。
///
/// 当 `SatControlCenter` 的后端从云端服务 (`SatCloudService`) 收到一个 Echo (回声) 消息的回复时，
/// 它会构建此结构体的实例，并将云端回复的原始文本内容填充到 `content` 字段中。
/// 然后，此实例将作为负载，通过名为 `ECHO_RESPONSE_EVENT` 的 Tauri 事件发送给前端应用，
/// 以便前端可以展示或处理这个回声响应。
/// 添加 `Deserialize` 是为了与 `SatOnSiteMobile` (现场移动端) 项目中的对应事件负载结构体保持一致性，
/// 尽管对于 `SatControlCenter` 作为事件的发送方，`Deserialize` 可能不是严格必需的，但保持一致性有助于减少潜在的混淆。
#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct EchoResponseEventPayload {
    /// 从云端服务回声响应中获取到的原始文本内容。
    pub content: String,
}

// 提示：后续可以根据 `SatControlCenter` 应用的特定业务需求，在此文件中定义更多应用级的 Tauri 事件常量和负载结构体。
// 例如，当 PLC 通信状态发生变化、或特定控制指令执行完成时，都可以定义相应的事件来通知前端。 