// SatCloudService/src-tauri/src/ws_server/message_router.rs

//! WebSocket 消息路由器模块。
//!
//! 本模块的核心功能是异步处理从已连接的 WebSocket 客户端接收到的各类消息。
//! 它扮演着服务端消息处理的中央分发枢纽角色：
//! - **接收与记录**: 接收来自 `WsService` 传递的原始 `WsMessage`。
//! - **更新活跃状态**: 每当收到客户端的任何消息时，都会更新该客户端会话 (`ClientSession`) 中的
//!   `last_seen` 时间戳，这对于 `HeartbeatMonitor` 的超时检测机制至关重要。
//! - **类型匹配与分发**: 根据消息的 `message_type` 字符串字段，将消息路由到相应的具体处理逻辑分支。
//! - **负载解析**: 对于每种已知的消息类型，尝试将其 `payload` (通常是JSON字符串) 反序列化为
//!   在 `common_models::ws_payloads` 中定义的对应强类型负载结构体。
//! - **业务逻辑调用**: 
//!   - 对于如 "Echo" (回声) 或 "Ping" (心跳) 这样的简单消息，直接在本模块内构造并发送响应。
//!   - 对于如 "Register" (客户端注册/加入组) 这样的复杂消息，会调用 `ConnectionManager` 的方法来处理。
//!   - (未来扩展，P3.3.2) 对于特定于业务领域（如调试任务操作）的消息，将调用 `TaskStateManager` 
//!     或其他专门的业务逻辑处理器来更新共享状态，并可能触发向同组伙伴客户端分发更新。
//! - **响应生成与发送**: 根据处理结果，构造适当的响应消息 (例如，`PongPayload` 对 "Ping"，
//!   `RegisterResponsePayload` 对 "Register"，或通用的 `ErrorResponsePayload` 对于错误情况)，
//!   然后通过客户端会话的 `sender` 将响应异步发送回原始请求的客户端。
//! - **错误处理与报告**: 对无法识别的消息类型、负载反序列化失败或其他处理错误，会记录详细的警告或错误日志，
//!   并通常会向客户端发送一个包含错误信息的标准 `ErrorResponsePayload`。

use std::sync::Arc; // 原子引用计数 Arc，用于在异步任务间安全地共享对象所有权，如 ClientSession, ConnectionManager 等。
use anyhow::Result; // anyhow 提供的 Result 类型，用于简化错误处理链，允许返回多种错误类型。
use log::{debug, warn, error, info}; // 标准日志宏，用于在不同级别记录程序运行信息。
use chrono::Utc; // chrono 库的 Utc 时间模块，用于获取当前协调世界时 (UTC) 时间戳。

use common_models::ws_payloads::{ // 从共享模型库引入 WebSocket 消息负载 (payload) 定义
    self, // 引入整个 ws_payloads 模块本身，使得可以通过 ws_payloads::CONSTANT_NAME 访问常量
    EchoPayload, // 用于 "Echo" (回声) 请求和响应的负载结构体定义。
    ErrorResponsePayload, // 用于向客户端发送标准格式错误信息的负载结构体定义。
    PingPayload, // 用于客户端 "Ping" (心跳) 请求的负载结构体定义 (P1.4.1 新增)。
    PongPayload, // 用于服务端对 "Ping" (心跳) 请求的 "Pong" 响应的负载结构体定义 (P1.4.1 新增)。
    RegisterPayload, // 用于客户端发起注册或加入调试任务组请求的负载结构体定义 (P3.1.2 新增)。
    RegisterResponsePayload, // 用于服务端对客户端注册/加入组请求的响应的负载结构体定义 (P3.1.2 新增)。
    // 提醒 (P3.3.2): 未来与具体业务逻辑相关的负载类型 (例如 UpdatePreCheckItemPayload, StartSingleTestStepPayload 等)
    // 也应在此处或相应的业务模型模块中定义，并可能需要在此 MessageRouter 中添加处理分支。
};
use rust_websocket_utils::message::WsMessage; // 从公司内部的 WebSocket 工具库引入标准 WebSocket 消息体结构定义。
use super::client_session::ClientSession; // 引入同一模块层级下的 `client_session` 子模块中定义的 `ClientSession` 结构体。
use super::connection_manager::ConnectionManager; // 引入同一模块层级下的 `connection_manager` 子模块中定义的 `ConnectionManager` 结构体 (P3.1.2 新增)。
// 提醒 (P3.3.2): 如果业务消息处理逻辑被封装在 TaskStateManager 或其他新模块中，需要在此处引入它们。
// use super::task_state_manager::TaskStateManager; // (示例性，待 P3.3.2 实现时取消注释并确认路径)

/// 异步处理从特定客户端接收到的单个 WebSocket 消息 (`WsMessage`)。
///
/// 此核心函数由 `WsService` (WebSocket 服务主监听和连接处理模块) 在其处理每个客户端连接的
/// 内部循环中调用。每当从客户端的 WebSocket 连接成功接收到一个完整的消息时，
/// `WsService` 就会将该消息以及相关的客户端会话信息传递给此 `handle_message` 函数进行处理。
///
/// # 主要职责
/// 1.  **更新客户端活跃时间**: 立即更新与该客户端关联的 `ClientSession` 中的 `last_seen` 时间戳，
///     表明客户端仍然活跃。这对 `HeartbeatMonitor` 的超时检测至关重要。
/// 2.  **消息路由**: 根据传入 `WsMessage` 的 `message_type` 字符串字段，将消息分发到
///     相应的处理逻辑分支（`match` 语句）。
/// 3.  **负载解析与业务处理**: 对于每种已知的消息类型：
///     a.  尝试将其 `payload` (通常是一个JSON字符串) 反序列化为在 `common_models::ws_payloads`
///         中定义的对应强类型负载结构体 (例如，`EchoPayload`, `RegisterPayload`)。
///     b.  如果解析成功，则执行与该消息类型相关的业务逻辑。这可能包括：
///         - 直接构造并发送响应 (如对 "Echo" (回声) 或 "Ping" (心跳) 消息)。
///         - 调用其他管理器 (如 `ConnectionManager` 处理 "Register" (注册) 消息，未来可能调用 `TaskStateManager` 
///           处理业务数据同步消息) 来执行更复杂的操作。
/// 4.  **响应生成与发送**: 根据业务逻辑的处理结果，创建一个新的 `WsMessage` 作为响应
///     (例如，`PongPayload` 作为对 "Ping" 的响应，`RegisterResponsePayload` 作为对 "Register" 的响应，
///     或一个通用的 `ErrorResponsePayload` 来指示错误)，然后通过 `client_session.sender` 
///     将此响应消息异步地发送回原始请求的客户端。
/// 5.  **错误处理与日志记录**: 
///     a.  如果遇到无法识别的 `message_type`，或者在尝试反序列化已知类型的 `payload` 时失败
///         (例如，JSON格式错误或字段不匹配)，会记录详细的警告或错误日志。
///     b.  在大多数错误情况下 (特别是负载解析失败或已知的业务逻辑错误)，会尝试向客户端
///         发送一个包含具体错误描述的 `ErrorResponsePayload` (或特定类型的失败响应，如 `RegisterResponsePayload` 中的 `success: false`)。
///     c.  即使向客户端发送响应消息时发生错误 (例如，客户端可能在服务器准备好响应之前就已意外断开连接)，
///         此函数通常也会仅记录该发送错误并继续正常返回 (`Ok(())`)，以避免单个客户端的问题
///         影响整个服务器的消息处理主循环。
///
/// # 参数
/// * `client_session`: `Arc<ClientSession>` - 对发送此消息的客户端的 `ClientSession` 实例的共享引用。
///   `ClientSession` 封装了客户端的唯一ID、网络地址、角色、所属组、最后活跃时间以及用于向其发送消息的通道 (`sender`)。
/// * `message`: `WsMessage` - 从客户端接收到的、需要被处理的实际 WebSocket 消息实例。
/// * `connection_manager`: `Arc<ConnectionManager>` - (P3.1.2 新增) 对 `ConnectionManager` 实例的共享引用。
///   `ConnectionManager` 负责管理客户端的组信息和注册流程，因此在处理如 "Register" (注册) 类型的消息时需要用到它。
/// * `_task_state_manager`: `Arc<TaskStateManager>` - (P3.3.2 规划中，目前用 `_` 忽略) 对 `TaskStateManager` 实例的共享引用。
///   未来当实现处理具体业务数据同步的消息类型时，将通过此参数与 `TaskStateManager` 交互。
///
/// # 返回值
/// * `Result<(), anyhow::Error>`: 
///   - `Ok(())`: 表示此消息的主要处理流程已完成，即使在处理过程中可能发生了可恢复的子错误
///     (例如，向特定客户端发送响应失败，但这不应停止对其他客户端消息的处理)。
///   - `Err(anyhow::Error)`: 表示在消息处理过程中发生了某种严重的、可能需要更高级别干预的错误。
///     然而，当前实现倾向于内部处理大多数错误并返回 `Ok(())`，以保持服务的健壮性。
///     返回 `Err` 的情况应非常罕见，通常只在遇到无法恢复的内部状态问题或配置错误时考虑。
pub async fn handle_message(
    client_session: Arc<ClientSession>,
    message: WsMessage,
    connection_manager: Arc<ConnectionManager>, // P3.1.2: 添加 ConnectionManager 作为参数
    // _task_state_manager: Arc<TaskStateManager>, // P3.3.2: 占位，未来用于业务消息处理
) -> Result<(), anyhow::Error> {
    // 步骤 1: 更新客户端会话的 `last_seen` 时间戳，记录其最近的活跃时间。
    // 这是心跳机制 (`HeartbeatMonitor`) 判断客户端是否超时的关键依据。
    let now = Utc::now(); // 获取当前的UTC时间。
    *client_session.last_seen.write().await = now; // 异步获取 `last_seen` 字段的写锁并更新其值。
    debug!(
        "[消息路由] 客户端 {} (地址: {})：已将其 last_seen (最后活跃) 时间戳更新为: {} (UTC)",
        client_session.client_id, client_session.addr, now.to_rfc3339() // 使用 RFC3339 格式化时间，更易读且标准化
    );

    // 记录接收到消息的基本信息，便于追踪和调试。
    info!(
        "[消息路由] 客户端 {} (地址: {})：接收到类型为 '{}' 的消息。",
        client_session.client_id, client_session.addr, message.message_type
    );
    // 仅在调试级别记录完整的原始负载，因为它可能包含敏感信息或过长的内容，不适合在 info 级别常规输出。
    debug!(
        "[消息路由] 客户端 {} (地址: {})：消息的原始负载 (JSON字符串): '{}'",
        client_session.client_id, client_session.addr, message.payload
    );

    // 步骤 2: 根据 `WsMessage` 中的 `message_type` 字符串，将消息路由到相应的处理分支。
    match message.message_type.as_str() { // 使用 as_str() 将 String 转换为 &str 以便匹配常量
        // 分支 2.1: 处理 "Echo" (回声) 类型的消息。
        // "Echo" (回声) 消息通常用于简单的连接测试，服务器会将其负载原样返回给客户端。
        ws_payloads::ECHO_MESSAGE_TYPE => { 
            info!(
                "[消息路由] 客户端 {} (地址: {})：正在处理 Echo (回声) 请求。",
                client_session.client_id, client_session.addr
            );
            // 尝试将消息的 JSON 负载反序列化为 `EchoPayload` 结构体。
            match serde_json::from_str::<EchoPayload>(&message.payload) {
                Ok(echo_payload) => { // 如果反序列化成功...
                    debug!(
                        "[消息路由] 客户端 {}：EchoPayload 解析成功。接收到的内容: '{}'",
                        client_session.client_id, echo_payload.content
                    );

                    // 准备 Echo (回声) 响应消息。对于 Echo，响应负载与请求负载相同。
                    // 使用 `ws_payloads::ECHO_MESSAGE_TYPE` 作为响应的消息类型。
                    match WsMessage::new(ws_payloads::ECHO_MESSAGE_TYPE.to_string(), &echo_payload) {
                        Ok(response_msg) => { // 如果成功创建了 `WsMessage` 实例...
                            // 通过此客户端会话的 `sender` (一个MPSC通道的发送端) 将响应消息异步发送回该客户端。
                            if let Err(e) = client_session.sender.send(response_msg).await {
                                // 如果发送失败 (例如，客户端的接收任务已关闭，或通道已满/关闭)，记录错误。
                                error!(
                                    "[消息路由] 客户端 {} (地址: {})：发送 Echo (回声) 响应消息失败: {}. 可能原因：客户端已断开连接，或其接收通道已关闭。",
                                    client_session.client_id, client_session.addr, e
                                );
                            } else {
                                // 如果发送成功，记录日志。
                                info!(
                                    "[消息路由] 客户端 {} (地址: {})：Echo (回声) 响应已成功发送。回显内容: '{}'",
                                    client_session.client_id, client_session.addr, echo_payload.content
                                );
                            }
                        }
                        Err(e) => { // 如果 `WsMessage::new` 创建响应消息失败 (理论上不应发生，除非负载序列化自身出问题)
                            error!(
                                "[消息路由] 客户端 {} (地址: {})：为 Echo (回声) 响应创建 WsMessage 实例时发生内部错误: {}. 原始请求负载: {:?}",
                                client_session.client_id, client_session.addr, e, echo_payload
                            );
                        }
                    }
                }
                Err(e) => { // 如果 `EchoPayload` 反序列化失败 (例如，JSON格式错误或字段不匹配)...
                    warn!(
                        "[消息路由] 客户端 {} (地址: {})：解析 Echo (回声) 请求的负载 (EchoPayload) 失败: {}. 原始JSON负载: '{}'",
                        client_session.client_id, client_session.addr, e, message.payload
                    );
                    // 向客户端发送一个标准的错误响应，告知其请求的负载无效。
                    send_error_response(
                        &client_session, // 目标客户端会话
                        Some(ws_payloads::ECHO_MESSAGE_TYPE.to_string()), // 指明原始请求的消息类型是 "Echo" (回声)
                        format!("Echo (回声) 请求的负载 (payload) 格式无效: {}. 请确保提供符合 EchoPayload 结构的JSON对象。", e), // 具体的错误信息
                    )
                    .await; // 等待错误响应发送完成（或失败）
                }
            }
        }

        // 分支 2.2 (P1.4.1 新增): 处理 "Ping" (心跳) 类型的消息。
        // 客户端会定期发送 "Ping" (心跳) 消息以表明其仍然活跃，并期望服务器回复 "Pong" (心跳响应)。
        ws_payloads::PING_MESSAGE_TYPE => { 
            info!(
                "[消息路由] 客户端 {} (地址: {})：收到 Ping (心跳) 请求。",
                client_session.client_id, client_session.addr
            );
            // `PingPayload` 当前定义为一个空结构体，但仍尝试进行反序列化，
            // 以保持处理流程的一致性，并为未来可能向 Ping 消息添加负载内容留出扩展空间。
            match serde_json::from_str::<PingPayload>(&message.payload) {
                Ok(_ping_payload) => { // 如果反序列化成功 (对于空结构体，通常意味着负载是 `{}` 或空的JSON兼容结构)
                                      // `_ping_payload` 当前未使用，所以用下划线 `_` 前缀忽略它以避免编译器警告。
                    debug!(
                        "[消息路由] 客户端 {}：PingPayload 解析成功 (由于其为空结构体，通常表示负载符合预期)。",
                        client_session.client_id
                    );
                    
                    // 准备 Pong (心跳响应) 消息。
                    let pong_payload = PongPayload {}; // `PongPayload` 当前也定义为一个空结构体。
                    // 使用 `ws_payloads::PONG_MESSAGE_TYPE` 作为响应的消息类型。
                    match WsMessage::new(ws_payloads::PONG_MESSAGE_TYPE.to_string(), &pong_payload) {
                        Ok(pong_msg) => { // 如果成功创建了 `WsMessage` 实例...
                            if let Err(e) = client_session.sender.send(pong_msg).await {
                                error!(
                                    "[消息路由] 客户端 {} (地址: {})：发送 Pong (心跳) 响应失败: {}. 可能原因：客户端已断开连接。",
                                    client_session.client_id, client_session.addr, e
                                );
                            } else {
                                info!(
                                    "[消息路由] 客户端 {} (地址: {})：Pong (心跳) 响应已成功发送。",
                                    client_session.client_id, client_session.addr
                                );
                            }
                        }
                        Err(e) => { // 如果 `WsMessage::new` 创建 Pong 响应消息失败...
                            error!(
                                "[消息路由] 客户端 {} (地址: {})：为 Pong (心跳) 响应创建 WsMessage 实例时发生内部错误: {}. 原始Ping请求负载: '{}'",
                                client_session.client_id, client_session.addr, e, message.payload
                            );
                        }
                    }
                }
                Err(e) => { // 如果 `PingPayload` 反序列化失败...
                    warn!(
                        "[消息路由] 客户端 {} (地址: {})：解析 Ping (心跳) 请求的负载 (PingPayload) 失败: {}. 原始JSON负载: '{}'. "
                        + "由于 Ping 消息的负载通常预期为空或非常简单，通常不对此类解析错误回复错误消息给客户端，以避免不必要的网络流量。",
                        client_session.client_id, client_session.addr, e, message.payload
                    );
                    // 对于 Ping 消息的负载解析失败，通常不建议向客户端发送 ErrorResponse，
                    // 因为这可能与某些 WebSocket 心跳实现（期望简单 Ping/Pong，不处理复杂错误）不兼容，
                    // 或者可能导致不必要的网络拥塞。如果需要严格的 Ping 格式验证并回复错误，
                    // 可以取消下面的注释以启用错误响应发送：
                    // send_error_response(
                    //     &client_session,
                    //     Some(ws_payloads::PING_MESSAGE_TYPE.to_string()), // 原始请求类型 "Ping" (心跳)
                    //     format!("Ping (心跳) 请求的负载 (payload) 格式无效: {}. 预期为空或简单JSON对象。", e),
                    // )
                    // .await;
                }
            }
        }

        // 分支 2.3 (P3.1.2 新增): 处理 "Register" (注册/加入组) 类型的消息。
        // 客户端通过此消息向服务器声明其角色，并请求加入一个特定的调试任务组。
        ws_payloads::REGISTER_MESSAGE_TYPE => { 
            info!(
                "[消息路由] 客户端 {} (地址: {})：收到 Register (注册/加入组) 请求。原始负载: '{}'",
                client_session.client_id, client_session.addr, message.payload
            );
            // 尝试将消息的 JSON 负载反序列化为 `RegisterPayload` 结构体。
            match serde_json::from_str::<RegisterPayload>(&message.payload) {
                Ok(parsed_payload) => { // 如果反序列化成功...
                    info!(
                        "[消息路由] 客户端 {}：RegisterPayload 解析成功。请求加入组ID: '{}', 声明角色: {:?}, 关联任务ID: '{}'",
                        client_session.client_id, parsed_payload.group_id, parsed_payload.role, parsed_payload.task_id
                    );

                    // 调用 `ConnectionManager` 的 `join_group` 方法来处理实际的注册和组加入逻辑。
                    // `join_group` 方法会负责：
                    // - 查找或创建具有指定 `group_id` 的组。
                    // - 检查声明的 `role` 在该组内是否可用 (例如，一个组通常只允许一个控制中心)。
                    // - 如果允许加入，则更新 `ClientSession` 和 `Group` 的状态。
                    // - 通知同组的伙伴客户端（如果存在）新成员的加入。
                    // - （P3.3.1 集成）通知 `TaskStateManager` 初始化与此组关联的任务状态。
                    let response_payload_result = connection_manager
                        .join_group(client_session.clone(), parsed_payload) // 将客户端会话的共享引用和解析后的负载传递给 join_group
                        .await; // `join_group` 是一个异步方法
                    
                    // `join_group` 方法返回一个 `Result<RegisterResponsePayload, RegisterResponsePayload>`。
                    // 这种设计意味着无论是业务上的成功 (例如，成功加入组) 还是可预期的业务失败 
                    // (例如，角色冲突导致无法加入)，都通过一个 `RegisterResponsePayload` 来向客户端传达结果。
                    // 我们需要将这个 Result 展平为单个 `RegisterResponsePayload` 以便发送。
                    let final_response_payload = match response_payload_result {
                        Ok(success_resp) => { // 如果 `join_group` 返回 Ok(payload)，表示操作成功。
                            info!(
                                "[消息路由] 客户端 {} (地址: {})：加入组操作已由 ConnectionManager 成功处理。准备发送成功的 RegisterResponse (注册响应)。响应详情: {:?}",
                                client_session.client_id, client_session.addr, success_resp
                            );
                            success_resp // 直接使用成功时的响应负载
                        },
                        Err(failure_resp) => { // 如果 `join_group` 返回 Err(payload)，表示发生了业务逻辑上的失败。
                            info!(
                                "[消息路由] 客户端 {} (地址: {})：加入组操作已被 ConnectionManager 判定为失败。准备发送失败的 RegisterResponse (注册响应)。响应详情: {:?}",
                                client_session.client_id, client_session.addr, failure_resp
                            );
                            failure_resp // 使用失败时的响应负载 (其中 success 字段应为 false)
                        }, 
                    };

                    // 根据 `join_group` 的处理结果 (封装在 `final_response_payload` 中)，
                    // 创建并发送 `RegisterResponse` (注册响应) 消息给原始请求的客户端。
                    match WsMessage::new(
                        ws_payloads::REGISTER_RESPONSE_MESSAGE_TYPE.to_string(), // 消息类型为 "RegisterResponse" (注册响应)
                        &final_response_payload, // 使用 `join_group` 返回的最终响应负载
                    ) {
                        Ok(response_ws_msg) => { // 如果成功创建了 `WsMessage` 实例...
                            if let Err(e) = client_session.sender.send(response_ws_msg).await {
                                error!(
                                    "[消息路由] 客户端 {} (地址: {})：发送 RegisterResponse (注册响应) 失败: {}. 可能原因：客户端已断开。响应负载详情: {:?}",
                                    client_session.client_id, client_session.addr, e, final_response_payload
                                );
                            } else {
                                info!(
                                    "[消息路由] 客户端 {} (地址: {})：RegisterResponse (注册响应) 已成功发送。响应中 success 标志为: {}.",
                                    client_session.client_id, client_session.addr, final_response_payload.success
                                );
                            }
                        }
                        Err(e) => { // 如果 `WsMessage::new` 创建 RegisterResponse 消息失败...
                            error!(
                                "[消息路由] 客户端 {} (地址: {})：为 RegisterResponse (注册响应) 创建 WsMessage 实例时发生内部错误: {}. 原始响应负载详情: {:?}",
                                client_session.client_id, client_session.addr, e, final_response_payload
                            );
                        }
                    }
                }
                Err(e) => { // 如果 `RegisterPayload` 反序列化失败 (例如，JSON格式错误或字段不匹配)...
                    warn!(
                        "[消息路由] 客户端 {} (地址: {})：解析 Register (注册/加入组) 请求的负载 (RegisterPayload) 失败: {}. 原始JSON负载: '{}'",
                        client_session.client_id, client_session.addr, e, message.payload
                    );
                    // 对于 RegisterPayload 解析失败的情况，我们也应该向客户端发送一个明确的失败响应，
                    // 使用 `RegisterResponsePayload` 结构并设置 `success` 为 `false`。
                    let error_response = RegisterResponsePayload {
                        success: false, // 明确指示操作失败
                        message: Some(format!("无效的 Register (注册/加入组) 请求负载格式: {}. 请确保提供符合 RegisterPayload 结构的JSON对象。", e)),
                        assigned_client_id: client_session.client_id, // 即使失败，也告知客户端其当前的会话ID，便于调试
                        effective_group_id: None, // 未能加入任何组
                        effective_role: None,     // 未能分配任何角色
                    };
                    // 尝试创建并发送这个包含解析错误的 RegisterResponse (注册响应) 消息。
                    match WsMessage::new(
                        ws_payloads::REGISTER_RESPONSE_MESSAGE_TYPE.to_string(), // 消息类型仍为 "RegisterResponse" (注册响应)
                        &error_response, // 使用我们构造的错误响应负载
                    ) {
                        Ok(response_ws_msg) => {
                            if let Err(send_err) = client_session.sender.send(response_ws_msg).await {
                                error!(
                                    "[消息路由] 客户端 {} (地址: {})：发送关于 RegisterPayload 解析失败的 RegisterResponse (注册响应) 时再次失败: {}. 原始错误负载: {:?}",
                                    client_session.client_id, client_session.addr, send_err, error_response
                                );
                            }
                        }
                        Err(create_err) => {
                             error!(
                                "[消息路由] 客户端 {} (地址: {})：为指示 RegisterPayload 解析失败的 RegisterResponse (注册响应) 创建 WsMessage 时发生内部错误: {}. 错误响应负载: {:?}",
                                client_session.client_id, client_session.addr, create_err, error_response
                            );
                        }
                    }
                }
            }
        }

        // TODO (P3.3.2 - 业务消息处理与状态同步):
        // 此处将是未来添加处理具体业务相关消息类型 (例如 "UpdatePreCheckItem" (更新预检项), "StartSingleTestStep" (开始单步测试), 
        // "FeedbackSingleTestStepResult" (反馈单步测试结果), "ConfirmSingleTestStep" (确认单步测试), "UpdateInterlockCondition" (更新联锁条件) 等) 的分支。
        // 
        // 对于每一种业务消息类型，其大致处理流程可能如下：
        // 1. **权限/状态检查 (可选但推荐)**:
        //    - 检查发起此操作的 `client_session.role` 是否有权限执行此操作。
        //    - 检查 `client_session.group_id` 是否有效，以及该组是否处于允许此操作的状态。
        // 2. **负载解析**: 将 `message.payload` 反序列化为该业务消息对应的特定 Payload 结构体 
        //    (例如，`UpdatePreCheckItemPayload`)。
        // 3. **调用 TaskStateManager**: 将解析后的 Payload 和相关上下文 (如 `group_id`, `client_role`)
        //    传递给 `TaskStateManager` 的某个方法 (例如，`task_state_manager.update_task_debug_state(...)`)。
        //    `TaskStateManager` 将负责根据输入更新其内部维护的对应任务 (`TaskDebugState`) 的权威数据模型。
        // 4. **处理 TaskStateManager 的返回值**: 
        //    - `TaskStateManager` 的更新方法可能会返回更新后的完整 `TaskDebugState`，或者一个指示是否发生变化的标志。
        //    - 如果状态确实发生了改变，则需要将这个更新后的状态通知给组内的伙伴客户端。
        // 5. **向伙伴客户端分发状态更新**:
        //    a. 使用 `ConnectionManager` 找到与当前 `client_session` 在同一组内的伙伴客户端的 `ClientSession`。
        //    b. 创建一个新的 `WsMessage`，其 `message_type` 应为一个专用的全局状态更新类型 
        //       (例如，`ws_payloads::TASK_STATE_UPDATE_MESSAGE_TYPE`，其值为 "TaskStateUpdate" (任务状态更新))。
        //    c. 将从 `TaskStateManager` 获取到的、已更新的完整 `TaskDebugState` 实例序列化为 JSON 字符串，
        //       作为此 "TaskStateUpdate" (任务状态更新) 消息的 `payload`。
        //    d. 通过伙伴客户端的 `sender` 将此 "TaskStateUpdate" (任务状态更新) 消息异步发送出去。
        // 6. **向原始请求客户端发送确认 (可选)**: 根据业务需求，可能需要向发起业务操作的客户端
        //    发送一个简单的确认消息 (例如，`{ "success": true, "message": "操作已处理" }`)，或者如果操作
        //    本身就是状态更新的一部分，则它也会收到上述的全局 "TaskStateUpdate" (任务状态更新) 消息，可能无需额外确认。
        // 7. **错误处理**: 对 Payload 解析失败、权限不足、或 `TaskStateManager` 返回业务错误等情况，
        //    应向原始请求客户端发送包含具体错误信息的 `ErrorResponsePayload`。
        //
        // 示例占位 (实际实现时会替换为具体的业务消息类型常量和逻辑):
        // ws_payloads::UPDATE_PRE_CHECK_ITEM_TYPE => { 
        //     info!("[消息路由] 客户端 {}：收到模拟的 更新预检查项 请求。", client_session.client_id);
        //     // ... 实现上述1-7的逻辑 ...
        //     // 例如: send_error_response(&client_session, Some("UpdatePreCheckItem".to_string()), "功能正在开发中，敬请期待。".to_string()).await;
        // }

        // 默认分支: 处理所有未被以上 `match`臂匹配到的未知消息类型。
        _ => { // `_` 是一个通配符，匹配任何其他字符串值
            warn!(
                "[消息路由] 客户端 {} (地址: {})：收到未知或当前不支持的消息类型: '{}'。原始负载: '{}'",
                client_session.client_id, client_session.addr, message.message_type, message.payload
            );
            // 向客户端发送一个标准的错误响应，告知其消息类型不被支持。
            send_error_response(
                &client_session, // 目标客户端会话
                Some(message.message_type.clone()), // 包含原始的、未被识别的消息类型
                format!("服务器不支持消息类型 '{}'，或者该类型当前未实现处理逻辑。", message.message_type),
            )
            .await; // 等待错误响应发送完成（或失败）
        }
    }

    Ok(()) // 表示此 `handle_message` 调用已成功完成其主要处理流程。
} // handle_message 函数结束

/// 辅助函数：向指定的客户端会话异步发送一个标准格式的错误响应消息。
///
/// 此函数封装了创建和发送 `ErrorResponsePayload` 的通用逻辑，简化了在多个错误处理点重复编写相似代码的需求。
///
/// # 参数
/// * `client_session`: `&Arc<ClientSession>` - 对目标客户端的 `ClientSession` 的共享引用。
///   错误响应将被发送到此会话所代表的客户端。
/// * `original_message_type`: `Option<String>` - 可选参数，如果错误是针对某个特定类型的原始请求消息，
///   则此参数应包含该原始消息的 `message_type` 字符串。这有助于客户端将错误与原始请求关联起来。
///   如果错误不是针对特定请求类型（例如，一个通用的连接错误），则可以传入 `None`。
/// * `error_message_text`: `String` - 描述错误的具体文本信息。此信息将包含在发送给客户端的
///   `ErrorResponsePayload` 的 `message` 字段中。
///
/// # 注意
/// 此函数会尝试发送错误响应，但如果发送本身失败 (例如，客户端已断开连接)，
/// 它会记录一个错误日志，但不会将此发送失败作为错误传播回调用者 (即，它不返回 `Result`)。
/// 这是为了避免因尝试报告一个错误而引发另一个需要处理的错误，从而简化上层错误处理逻辑。
async fn send_error_response(
    client_session: &Arc<ClientSession>,    // 目标客户端会话
    original_message_type: Option<String>, // 可选的原始消息类型，用于帮助客户端关联错误来源
    error_message_text: String,            // 描述错误的具体文本信息
) {
    // 构造标准错误响应负载 (ErrorResponsePayload)
    let error_payload = ErrorResponsePayload {
        success: false, // 明确指示操作/请求失败
        message: error_message_text.clone(), // 包含具体的错误描述文本
        request_type: original_message_type, // 包含与此错误相关的原始请求消息类型 (如果提供)
    };

    info!(
        "[消息路由::错误响应] 准备向客户端 {} (地址: {}) 发送错误响应。错误信息: '{}', 原始请求类型: {:?}",
        client_session.client_id, client_session.addr, error_message_text, error_payload.request_type
    );

    // 尝试创建包含此错误负载的 WebSocket 消息 (WsMessage)
    // 使用 `ws_payloads::ERROR_RESPONSE_MESSAGE_TYPE` 作为标准错误响应的消息类型。
    match WsMessage::new(ws_payloads::ERROR_RESPONSE_MESSAGE_TYPE.to_string(), &error_payload) {
        Ok(error_ws_msg) => { // 如果 WsMessage 创建成功...
            // 尝试通过客户端会话的 sender 将此错误消息异步发送出去。
            if let Err(e) = client_session.sender.send(error_ws_msg).await {
                // 如果发送错误响应本身也失败了 (例如，客户端恰好在此时断开连接)，
                // 则记录一个更严重的错误，因为我们未能通知客户端发生了问题。
                error!(
                    "[消息路由::错误响应] 向客户端 {} (地址: {}) 发送错误响应消息时再次失败: {}. "
                    + "原始错误是: '{}'. 客户端可能已断开连接，无法接收此错误通知。",
                    client_session.client_id, client_session.addr, e, error_message_text
                );
            } else {
                // 如果错误响应成功发送，记录日志。
                debug!(
                    "[消息路由::错误响应] 已成功向客户端 {} (地址: {}) 发送错误响应。错误: '{}'",
                    client_session.client_id, client_session.addr, error_message_text
                );
            }
        }
        Err(e) => { // 如果 `WsMessage::new` 创建错误响应消息本身失败 (这通常表示内部序列化问题，非常罕见)
            error!(
                "[消息路由::错误响应] 为向客户端 {} (地址: {}) 发送错误通知而创建 WsMessage (错误类型) 时发生内部严重错误: {}. "
                + "原始要报告的错误是: '{}'. 序列化负载详情: {:?}",
                client_session.client_id, client_session.addr, e, error_message_text, error_payload
            );
        }
    }
} 