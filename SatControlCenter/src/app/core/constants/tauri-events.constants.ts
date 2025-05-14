// Tauri 事件名称常量，用于 SatOnSiteMobile 前端与 Rust 后端进行事件通信。
// 这些常量定义了事件的唯一字符串标识符，确保前后端使用一致的事件名称。

/**
 * WebSocket 伙伴客户端状态更新事件的名称。
 *
 * 当同一任务组内的伙伴客户端（例如中心端 `ControlCenter`）上线或下线，
 * 或者其状态发生其他重要变化时，Rust 后端会发送此事件。
 * 事件的负载 (payload) 通常包含伙伴的角色、在线状态以及客户端ID等信息。
 * 前端可以监听此事件以实时更新伙伴列表或相关UI元素。
 * 对应 Rust 结构体: `WsPartnerStatusEventPayload` (在 Rust 端的 `event.rs` 中定义)
 */
export const WS_PARTNER_STATUS_EVENT = 'ws_partner_status_event';

/**
 * WebSocket 连接状态事件的名称。
 *
 * 当应用与 WebSocket 云服务的连接状态发生变化时（例如，成功连接、断开连接、连接失败），
 * Rust 后端会发送此事件。事件的负载通常包含新的连接状态 (connected: boolean)
 * 以及可选的客户端ID (client_id) 或错误消息 (error_message)。
 * 前端监听此事件以更新UI上的连接指示器或执行相应的重连逻辑。
 * 对应 Rust 结构体: `WsConnectionStatusEventPayload`
 */
export const WS_CONNECTION_STATUS_EVENT = 'ws_connection_status';

/**
 * WebSocket 客户端注册状态事件的名称。
 *
 * 当客户端尝试向云服务注册其身份和任务信息后，Rust 后端会通过此事件
 * 将注册操作的结果（成功或失败、相关消息、分配的ID等）通知给前端。
 * 对应 Rust 结构体: `WsRegistrationStatusEventPayload`
 */
export const WS_REGISTRATION_STATUS_EVENT = 'ws_registration_status_event';

/**
 * 本地任务调试状态 (TaskDebugState) 更新事件的名称。
 *
 * 当云端权威的 `TaskDebugState` 发生变化时（例如，由于本地操作或伙伴客户端的操作），
 * Rust 后端服务会将更新后的完整 `TaskDebugState` 通过此事件发送给前端。
 * 前端接收到此事件后，应使用新的状态更新本地缓存和相关UI显示。
 * 对应 Rust 结构体: `LocalTaskStateUpdatedEventPayload` (其 `new_state` 字段为 `TaskDebugState`)
 */
export const LOCAL_TASK_STATE_UPDATED_EVENT = 'local_task_state_updated_event';

/**
 * Echo (回声) 响应事件的名称。
 *
 * 当客户端通过 Tauri 命令发送 Echo 请求到云端，并且云端成功返回 Echo 响应后，
 * Rust 后端会将收到的 Echo 内容通过此事件传递给前端。
 * 用于测试 WebSocket 通信链路。
 * 对应 Rust 结构体: `EchoResponseEventPayload`
 */
export const ECHO_RESPONSE_EVENT = 'echo_response_event';

// 开发者可以在此根据 SatOnSiteMobile 应用的特定需求，继续添加其他 Tauri 事件常量。
// 例如，如果增加了特定的硬件交互事件或移动端特有的通知事件，都可以在这里定义。
// 保持命名规范和详细注释，有助于团队协作和后续维护。 