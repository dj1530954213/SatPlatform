// SatCloudService/src-tauri/src/ws_server/connection_manager.rs

//! WebSocket 连接与客户端组管理模块。
//!
//! 该模块是 WebSocket 服务端的核心组件之一，主要职责包括：
//! - **客户端会话管理**: 跟踪所有通过 WebSocket 连接到服务器的活动客户端，
//!   每个客户端由一个 `ClientSession` 实例表示，存储在并发安全的哈希映射中。
//! - **组的创建与管理**: 允许客户端创建或加入特定的"组"（`Group`）。一个组通常对应一个
//!   正在进行的调试/测试任务，包含一个控制中心 (ControlCenter) 客户端和一个现场移动端 (OnSiteMobile) 客户端。
//!   组信息也存储在并发安全的哈希映射中。
//! - **角色分配与限制**: 在客户端加入组时，根据其声明的角色 (`ClientRole`) 将其分配到组内的
//!   特定槽位 (例如，一个组只能有一个 `ControlCenter` 和一个 `OnSiteMobile`)。
//! - **生命周期处理**: 处理客户端的连接 (`add_client`) 和断开 (`remove_client`) 事件。
//!   当客户端断开时，会将其从其所在的组中移除。
//! - **伙伴状态通知**: 当一个客户端加入或离开组时，会通知其在同一组内的伙伴客户端
//!   关于其在线状态的变化 (通过 `PartnerStatusPayload` 消息)。
//! - **任务状态关联**: 与 `TaskStateManager` 模块协作，在组创建时初始化与该组关联的
//!   任务状态 (`TaskDebugState`)，并在组解散时清理该状态。

use crate::ws_server::client_session::ClientSession;
use crate::ws_server::task_state_manager::TaskStateManager; // 引入任务状态管理器
use common_models::enums::ClientRole; // 引入客户端角色枚举
use common_models::ws_payloads::{ // 引入WebSocket消息负载定义
    PartnerStatusPayload, RegisterPayload, 
    PARTNER_STATUS_UPDATE_MESSAGE_TYPE,
    // REGISTER_RESPONSE_MESSAGE_TYPE, // P3.1.2: 确保 RegisterResponse 类型被引入 -- 这个常量确实未被使用
};
use common_models::RegisterResponsePayload; // 新增导入
use rust_websocket_utils::message::WsMessage; // 引入基础 WebSocket 消息结构

use dashmap::DashMap; // 高性能并发哈希映射库
use log::{debug, error, info, warn}; // 日志宏
use std::sync::Arc; // 原子引用计数，用于共享所有权
use tokio::sync::RwLock; // 异步读写锁，用于保护共享数据的并发访问
use uuid::Uuid; // 用于生成和操作 UUID
use std::net::SocketAddr; // 套接字地址类型
use tokio::sync::mpsc; // Tokio 提供的多生产者单消费者异步通道
use std::sync::atomic::{AtomicBool, Ordering}; // 原子布尔型及内存顺序控制

/// 代表一个客户端组，通常关联到一个特定的调试或协作任务。
/// 
/// 一个组设计为包含一个控制中心 (`ControlCenter`) 客户端和一个现场移动端 (`OnSiteMobile`) 客户端，
/// 它们共同参与完成由 `task_id` 标识的任务。
/// `Group` 实例由 `ConnectionManager` 创建和管理，并通过 `RwLock` 进行并发访问保护。
#[derive(Debug)] // 允许使用 {:?} 格式化打印 Group 以进行调试
pub struct Group {
    /// 组的唯一标识符字符串，由客户端在注册时提供或由系统生成。
    pub group_id: String,
    /// 与此组紧密关联的任务的唯一标识符字符串。
    /// 此 ID 在组创建时（通常由第一个加入的客户端的 `RegisterPayload` 提供）设定，
    /// 用于后续在 `TaskStateManager` 中查找和管理此任务相关的共享状态数据 (`TaskDebugState`)。
    pub task_id: String,
    /// 组内的控制中心 (`ClientRole::ControlCenter`) 客户端会话的共享引用。
    /// 使用 `Option<Arc<ClientSession>>` 表示该角色的客户端可能当前在线 (Some) 或离线/未加入 (None)。
    /// `Arc` 允许多处共享对 `ClientSession` 的只读访问。
    pub control_center_client: Option<Arc<ClientSession>>,
    /// 组内的现场移动端 (`ClientRole::OnSiteMobile`) 客户端会话的共享引用。
    /// 结构与 `control_center_client` 类似。
    pub on_site_mobile_client: Option<Arc<ClientSession>>,
    // 注意：未来可以扩展此结构以支持更多类型的客户端或观察者模式，
    // 例如 `additional_observers: Vec<Arc<ClientSession>>`。
}

impl Group {
    /// 创建一个新的 `Group` 实例。
    /// 
    /// 此构造函数在 `ConnectionManager` 的 `join_group` 方法中，当一个客户端尝试加入
    /// 一个尚不存在的组ID时被调用。新创建的组中，控制中心和现场移动端客户端均为空。
    ///
    /// # 参数
    /// * `group_id`: `String` - 新组的唯一标识符。
    /// * `task_id`: `String` - 与此组关联的任务的唯一标识符。
    ///
    /// # 返回值
    /// 返回一个初始化后的 `Group` 实例，其中 `control_center_client` 和 `on_site_mobile_client`
    /// 字段初始值均为 `None`。
    pub fn new(group_id: String, task_id: String) -> Self {
        info!("[连接管理器::组] 正在创建新的客户端组。组ID: '{}', 关联任务ID: '{}'", group_id, task_id);
        Self {
            group_id, // 设置组ID
            task_id,  // 设置关联的任务ID
            control_center_client: None, // 初始时无控制中心客户端
            on_site_mobile_client: None, // 初始时无现场移动端客户端
        }
    }
}

/// `ConnectionManager` 负责集中管理所有活动的 WebSocket 客户端会话 (`ClientSession`)
/// 以及客户端所组成的逻辑分组 (`Group`)。
/// 
/// 它是服务端 WebSocket 核心逻辑的重要组成部分，协调客户端的连接、断开、
/// 组的创建与加入、角色分配、以及与 `TaskStateManager` 交互以管理任务状态等。
/// 设计上使用了 `Arc` 和 `DashMap` 来确保其主要数据成员的线程安全和高效并发访问。
#[derive(Clone)] // 允许创建 ConnectionManager 的克隆副本 (主要是为了 Arc<ConnectionManager> 的克隆需求)
pub struct ConnectionManager {
    /// 存储所有当前活动的客户端会话的并发哈希映射。
    /// - 键 (`Uuid`): 每个客户端的唯一标识符 (`client_id`)。
    /// - 值 (`Arc<ClientSession>`): 对该客户端会话对象的共享引用。
    /// `DashMap` 提供了高效的并发读写能力。
    clients: Arc<DashMap<Uuid, Arc<ClientSession>>>,
    
    /// 存储所有当前活动的客户端组的并发哈希映射。
    /// - 键 (`String`): 每个组的唯一标识符 (`group_id`)。
    /// - 值 (`Arc<RwLock<Group>>`): 对该组对象的共享引用，并通过 `RwLock` 保护组内部成员的并发修改。
    groups: Arc<DashMap<String, Arc<RwLock<Group>>>>,
    
    /// 对任务状态管理器 (`TaskStateManager`) 的共享引用。
    /// `ConnectionManager` 使用它来在组创建时初始化与该组关联的任务的共享状态 (`TaskDebugState`)，
    /// 并在组解散（例如，最后一个成员离开组）时通知 `TaskStateManager` 清理相关状态。
    task_state_manager: Arc<TaskStateManager>,
}

impl ConnectionManager {
    /// 创建一个新的 `ConnectionManager` 实例。
    /// 
    /// 此构造函数应在服务启动时被调用一次，以初始化连接和组管理的中心枢纽。
    ///
    /// # 参数
    /// * `task_state_manager`: `Arc<TaskStateManager>` - 对 `TaskStateManager` 实例的共享引用，
    ///   用于在组的生命周期事件中协调任务状态的管理。
    ///
    /// # 返回值
    /// 返回一个初始化完成的 `ConnectionManager` 实例，其内部的 `clients` 和 `groups` 集合为空。
    pub fn new(task_state_manager: Arc<TaskStateManager>) -> Self {
        info!("[连接管理器] 正在创建并初始化一个新的 ConnectionManager 实例...");
        Self {
            clients: Arc::new(DashMap::new()), // 初始化空的客户端会话映射
            groups: Arc::new(DashMap::new()),  // 初始化空的客户端组映射
            task_state_manager,              // 存储对任务状态管理器的引用
        }
    }

    /// 将一个新的客户端会话添加到连接管理器中进行跟踪。
    /// 
    /// 此方法通常在 WebSocket 服务成功接受一个新的客户端连接后被调用 (例如，在 `WsService` 内部)。
    /// 它会创建一个新的 `ClientSession` 实例，并将其存储到内部的 `clients` 映射中。
    ///
    /// # 参数
    /// * `addr`: `SocketAddr` - 新连接客户端的网络套接字地址 (IP和端口)。
    /// * `sender`: `mpsc::Sender<WsMessage>` - 一个 Tokio MPSC 通道的发送端，专门用于将出站的
    ///   WebSocket 消息 (`WsMessage`) 异步地发送给这个新客户端。
    /// * `connection_should_close`: `Arc<AtomicBool>` - 一个共享的原子布尔标志。
    ///   外部模块 (如 `HeartbeatMonitor` 或 `ConnectionManager` 自身在移除客户端时)
    ///   可以通过设置此标志为 `true` 来请求关闭与此会话关联的底层 WebSocket 连接。
    ///
    /// # 返回值
    /// 返回对新创建并已添加的 `ClientSession` 实例的共享引用 (`Arc<ClientSession>`)。
    pub async fn add_client(
        &self,
        addr: SocketAddr,
        sender: mpsc::Sender<WsMessage>,
        connection_should_close: Arc<AtomicBool>,
    ) -> Arc<ClientSession> {
        // 创建一个新的 ClientSession 实例。ClientSession::new 内部会为其生成一个唯一的 client_id。
        let client_session = Arc::new(ClientSession::new(
            addr,                    // 客户端网络地址
            sender,                  // 用于向此客户端发送消息的通道
            connection_should_close, // 连接关闭标志
        ));
        
        // 将新创建的客户端会话插入到全局的 `clients` 映射中。
        // 使用 Arc::clone 来增加对 client_session 的引用计数，因为我们将同时在映射中存储它并返回它。
        self.clients
            .insert(client_session.client_id, Arc::clone(&client_session));
        
        // 获取客户端的初始角色以用于日志记录 (在 ClientSession::new 中默认为 Unknown)
        let initial_role = client_session.role.read().await.clone();
        info!(
            "[连接管理器] 新客户端已成功连接并添加至管理器进行跟踪。ID: {}, 地址: {}, 初始角色: {:?}",
            client_session.client_id, client_session.addr, initial_role
        );
        debug!("[连接管理器] 新增客户端会话详细信息: {:?}", client_session);
        debug!("[连接管理器] 当前活动客户端总数: {}", self.clients.len());

        client_session // 返回对新客户端会话的共享引用
    }

    /// 从连接管理器中移除一个指定的客户端会话，并处理其在组内的状态变更。
    /// 
    /// 此方法可能由多种原因触发，例如客户端主动断开连接、心跳超时检测到客户端无响应、
    /// 或其他网络错误导致连接终止。
    /// 
    /// 主要步骤包括：
    /// 1. 从 `clients` 映射中移除指定的 `client_id`。
    /// 2. 设置 `ClientSession` 中的 `connection_should_close` 标志为 `true`，以通知
    ///    处理该连接I/O的异步任务应终止并关闭物理连接。
    /// 3. 如果被移除的客户端之前已加入某个组：
    ///    a. 从该组中移除此客户端的引用。
    ///    b. 通知该组内的伙伴客户端（如果存在）此客户端已下线。
    ///    c. 如果移除此客户端后该组变为空，则从 `groups` 映射中移除该组，并通知
    ///       `TaskStateManager` 清理与该组关联的任务状态。
    ///
    /// # 参数
    /// * `client_id`: `&Uuid` - 要移除的客户端的唯一ID。
    pub async fn remove_client(&self, client_id: &Uuid) {
        info!("[连接管理器] 尝试从管理器中移除客户端: {}...", client_id);

        // 尝试从 `clients` 映射中移除指定的客户端会话。
        // `DashMap::remove` 返回一个 Option<(K, V)>，如果键存在则包含键值对。
        if let Some((_removed_id, client_session)) = self.clients.remove(client_id) {
            info!(
                "[连接管理器] 客户端 {} 已成功从活动客户端列表中找到并移除。",
                client_id
            );
            
            // 请求关闭与此会话关联的物理 WebSocket 连接。
            // 通过将 `ClientSession` 内共享的 `connection_should_close` 原子布尔标志设置为 `true`，
            // 来通知负责处理此连接I/O的异步任务（通常在 WsService 模块中）应该终止其读写循环并关闭连接。
            // `Ordering::SeqCst` 提供最强的内存顺序保证，确保此更改对其他线程可见。
            client_session
                .connection_should_close
                .store(true, Ordering::SeqCst);
            
            // 主动让出当前异步任务的执行权，允许 Tokio 的调度器运行其他准备就绪的任务。
            // 这可以帮助（但不保证）WsService 中的连接处理循环能更快地观察到 `connection_should_close` 标志的变化，
            // 从而加速物理连接的关闭过程。
            tokio::task::yield_now().await;

            debug!(
                "[连接管理器] 已成功请求关闭客户端 {} (地址: {}) 的底层 WebSocket 连接。",
                client_id, client_session.addr
            );

            // 获取客户端在断开连接前的角色和所属组ID，这些信息对于后续的组清理和通知逻辑至关重要。
            // 需要异步读取，因为它们被 RwLock 保护。
            let role_at_disconnect = client_session.role.read().await.clone();
            let group_id_option = client_session.group_id.read().await.clone();

            // 检查客户端是否属于某个组。
            if let Some(group_id) = group_id_option { 
                // 如果客户端的角色仍然是 Unknown，即使它有关联的 group_id (理论上不应发生这种情况，因为加入组时会设置角色)，
                // 也认为它未有效参与组，无需进行复杂的组内清理。
                if role_at_disconnect == ClientRole::Unknown {
                    info!("[连接管理器] 客户端 {} (角色: 未知) 在断开时虽有关联的组ID '{}'，但未被视为有效组成员，无需进行组内清理。", client_id, group_id);
                } else {
                    // 客户端属于一个已知的组，并且具有有效角色。
                    info!(
                        "[连接管理器] 客户端 {} (角色: {:?}) 在断开时属于组 '{}'。正在处理其在组内的移除及伙伴通知逻辑...",
                        client_id, role_at_disconnect, group_id
                    );
                    
                    // 尝试获取该组的锁以进行修改。
                    // `DashMap::get` 返回的是对 `Arc<RwLock<Group>>` 的引用，如果组存在的话。
                    if let Some(group_entry) = self.groups.get(&group_id) {
                        let group_lock = group_entry.value(); // 获取 Arc<RwLock<Group>>
                        let mut group = group_lock.write().await; // 获取组的异步写锁，准备修改组内成员
                        info!(
                            "[连接管理器::组处理] 正在为组 '{}' (任务ID: '{}') 处理客户端 {} (角色: {:?}) 的移除操作。",
                            group.group_id, group.task_id, client_id, role_at_disconnect
                        );

                        let mut partner_session_to_notify: Option<Arc<ClientSession>> = None; // 用于存储可能需要被通知的伙伴会话

                        // 根据被移除客户端的角色，将其从组内对应槽位移除，并确定其伙伴（如果存在）。
                        match role_at_disconnect {
                            ClientRole::ControlCenter => {
                                // 检查被移除的是否确实是当前组内的控制中心客户端。
                                if group.control_center_client.as_ref().map_or(false, |cs| cs.client_id == *client_id) {
                                    group.control_center_client = None; // 从组中移除控制中心客户端的引用
                                    partner_session_to_notify = group.on_site_mobile_client.as_ref().map(Arc::clone); // 伙伴是现场移动端
                                    info!(
                                        "[连接管理器::组处理] 客户端 {} (控制中心) 已从组 '{}' 中移除。",
                                        client_id, group.group_id
                                    );
                                } else {
                                    warn!(
                                        "[连接管理器::组处理] 客户端 {} (声明为控制中心) 在尝试从组 '{}' 移除时，发现其并非该组记录的控制中心客户端。可能状态不一致或重复移除。",
                                        client_id, group.group_id
                                    );
                                }
                            }
                            ClientRole::OnSiteMobile => {
                                // 检查被移除的是否确实是当前组内的现场移动端客户端。
                                if group.on_site_mobile_client.as_ref().map_or(false, |cs| cs.client_id == *client_id) {
                                    group.on_site_mobile_client = None; // 从组中移除现场移动端客户端的引用
                                    partner_session_to_notify = group.control_center_client.as_ref().map(Arc::clone); // 伙伴是控制中心
                                    info!(
                                        "[连接管理器::组处理] 客户端 {} (现场移动端) 已从组 '{}' 中移除。",
                                        client_id, group.group_id
                                    );
                                } else {
                                    warn!(
                                        "[连接管理器::组处理] 客户端 {} (声明为现场移动端) 在尝试从组 '{}' 移除时，发现其并非该组记录的现场移动端客户端。可能状态不一致或重复移除。",
                                        client_id, group.group_id
                                    );
                                }
                            }
                            ClientRole::Unknown => {
                                // Unknown 角色理论上不应出现在这里，因为前面已过滤。但为完整性保留。
                                warn!(
                                    "[连接管理器::组处理] 客户端 {} (角色: 未知) 正在被从组 '{}' 中处理移除，此情况非预期。",
                                    client_id, group.group_id
                                );
                            }
                        }

                        // 如果找到了伙伴，则向其发送关于当前客户端下线的通知。
                        if let Some(partner_session) = partner_session_to_notify {
                            let partner_status_payload = PartnerStatusPayload {
                                partner_role: role_at_disconnect.clone(), // 下线的是刚被移除的客户端的角色
                                partner_client_id: *client_id,            // 下线的是刚被移除的客户端的ID
                                is_online: false,                         // 状态是下线
                                group_id: group.group_id.clone(),         // 相关的组ID
                            };
                            match WsMessage::new(PARTNER_STATUS_UPDATE_MESSAGE_TYPE.to_string(), &partner_status_payload) {
                                Ok(ws_message) => {
                                    if let Err(e) = partner_session.sender.send(ws_message).await {
                                        error!(
                                            "[连接管理器::组处理] 向客户端 {} (伙伴 of {}) 发送关于客户端 {} (角色: {:?}) 下线的通知失败: {}。该伙伴可能也已断开。",
                                            partner_session.client_id, client_id, client_id, role_at_disconnect, e
                                        );
                                    } else {
                                        info!(
                                            "[连接管理器::组处理] 已成功向客户端 {} (伙伴 of {}) 发送了关于客户端 {} (角色: {:?}) 下线的通知。",
                                            partner_session.client_id, client_id, client_id, role_at_disconnect
                                        );
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        "[连接管理器::组处理] 创建伙伴下线通知 WsMessage 失败: {}. Payload: {:?}",
                                        e, partner_status_payload
                                    );
                                }
                            }
                        } else {
                            info!(
                                "[连接管理器::组处理] 客户端 {} (角色: {:?}) 从组 '{}' 移除后，该组内无其他伙伴需要通知。",
                                client_id, role_at_disconnect, group.group_id
                            );
                        }
                        
                        // 检查移除此客户端后，组是否变为空。
                        // 组变为空的条件是：控制中心客户端和现场移动端客户端均不存在 (None)。
                        let is_group_now_empty = group.control_center_client.is_none() && group.on_site_mobile_client.is_none();
                        
                        // 克隆需要在 drop(group) 之后使用的值
                        let group_id_for_cleanup = group.group_id.clone();
                        let task_id_for_cleanup = group.task_id.clone(); // 确保 task_id 也被克隆

                        if is_group_now_empty {
                            info!(
                                "[连接管理器::组处理] 组 '{}' (任务ID: '{}') 在客户端 {} 移除后已变为空。即将调用 TaskStateManager 清理其任务状态...",
                                group_id_for_cleanup, task_id_for_cleanup, client_id
                            );

                            // TaskStateManager 是 ConnectionManager 的一个成员 (Arc<TaskStateManager>)，因此它总是存在的。
                            // 直接使用 self.task_state_manager 来调用其方法。
                            if let Err(e) = self.task_state_manager.remove_task_state(&group_id_for_cleanup).await {
                                error!(
                                    "[连接管理器::组处理] 调用 TaskStateManager::remove_task_state 为组 '{}' 清理任务状态时发生错误: {:?}",
                                    group_id_for_cleanup, e
                                );
                            } else {
                                info!(
                                    "[连接管理器::组处理] 已成功调用 TaskStateManager::remove_task_state 为组 '{}' 清理任务状态。",
                                    group_id_for_cleanup
                                );
                            }

                            // 从 ConnectionManager 内部移除空组
                            if self.groups.remove(&group_id_for_cleanup).is_some() {
                                info!("[连接管理器::组处理] 空组 '{}' 已成功从 ConnectionManager 中移除。", group_id_for_cleanup);
                            } else {
                                warn!("[连接管理器::组处理] 尝试从 ConnectionManager 中移除空组 '{}'，但未找到该组。", group_id_for_cleanup);
                            }
                        }

                        // 在这里显式释放写锁，因为后续的 self.groups.remove 和 self.task_state_manager 调用不应持有单个组的锁。
                        drop(group); // group 的生命周期在此结束, group 在此之后不能再被直接使用

                    } else { // group_id 存在于 client_session 中，但在 self.groups 中未找到该组
                        warn!(
                            "[连接管理器] 客户端 {} (角色: {:?}) 声称属于组 '{}'，但在管理器中未找到该组。无法执行组内清理。",
                            client_id, role_at_disconnect, group_id
                        );
                         // P3.3.1 (考虑): 即使组在 ConnectionManager 中找不到了，但 TaskStateManager 中可能仍有残留状态。
                         // 是否需要根据 group_id尝试调用 task_state_manager.remove_task_state(&group_id).await;？
                         // 当前：不调用，因为组的权威记录在 ConnectionManager。如果 CM 中没有组，TSM 中也不应有活跃状态。
                         // （除非存在不一致的情况，这需要更深层次的错误恢复机制）
                    }
                } // 结束 else (role_at_disconnect != ClientRole::Unknown)
            } else { // client_session.group_id 为 None
                info!(
                    "[连接管理器] 客户端 {} 在断开时未属于任何组。无需进行组相关清理。",
                    client_id
                );
            }
            info!(
                "[连接管理器] 客户端 {} (原角色: {:?}) 的移除处理流程已完成。",
                client_id, role_at_disconnect
            );
        } else { // self.clients.remove(client_id) 返回 None
            warn!(
                "[连接管理器] 尝试移除客户端 {} 时失败：该客户端未在活动客户端列表中找到。可能已被移除或从未添加。",
                client_id
            );
        }
        debug!("[连接管理器] 当前活动客户端总数: {}", self.clients.len());
        debug!("[连接管理器] 当前活动组总数: {}", self.groups.len());
    }

    /// 处理客户端加入组的请求。
    ///
    /// 此方法由 `MessageRouter` 在收到类型为 "Register" 的 WebSocket 消息后调用。
    /// 主要职责包括：
    /// 1.  验证注册请求的有效性 (例如，角色是否允许加入目标组)。
    /// 2.  查找或创建目标组 (`Group`)。
    /// 3.  将客户端会话 (`ClientSession`) 添加到组中对应的角色槽位。
    /// 4.  更新客户端会话自身的 `role` 和 `group_id` 状态。
    /// 5.  如果组是新创建的，或这是第一个"有意义"的客户端加入，则通知 `TaskStateManager` 初始化关联的任务状态。
    /// 6.  向新加入的客户端发送其伙伴（如果已存在于组中）的在线状态。
    /// 7.  向组内已存在的伙伴客户端（如果存在）通知新客户端已上线。
    /// 8.  向请求客户端回复一个 `RegisterResponsePayload`，指示操作结果。
    ///
    /// # 参数
    /// * `client_session`: `Arc<ClientSession>` - 发起注册请求的客户端的会话对象。
    /// * `payload`: `RegisterPayload` - 从客户端 "Register" 消息中解析出的负载数据，
    ///   包含期望加入的 `group_id`、声明的 `role` 以及关联的 `task_id`。
    ///
    /// # 返回值
    /// 返回一个 `Result<RegisterResponsePayload, RegisterResponsePayload>`：
    /// - `Ok(payload)`: 表示注册成功，`payload` 包含了成功的信息和分配的角色/组。
    /// - `Err(payload)`: 表示注册失败，`payload` 包含了失败的原因。
    /// 这种返回类型允许调用者（`MessageRouter`）统一处理并向客户端发送响应。
    pub async fn join_group(
        &self,
        client_session: Arc<ClientSession>,
        payload: RegisterPayload,
    ) -> Result<RegisterResponsePayload, RegisterResponsePayload> {
        let client_id = client_session.client_id; // 获取客户端ID，用于日志和响应
        let requested_role = payload.role.clone(); // 克隆请求的角色
        let group_id = payload.group_id.clone();   // 克隆请求的组ID
        let task_id = payload.task_id.clone();     // 克隆请求的任务ID (P3.1.1)

        info!(
            "[连接管理器::注册] 客户端 {} 请求加入组 '{}' (任务ID: '{}')，声明角色为: {:?}。",
            client_id, group_id, task_id, requested_role
        );

        // 验证 task_id 是否为空，如果为空则拒绝注册 (基本校验)
        if task_id.is_empty() {
            warn!(
                "[连接管理器::注册] 客户端 {} 尝试注册到组 '{}' 时提供的 task_id 为空。注册被拒绝。",
                client_id, group_id
            );
            return Err(RegisterResponsePayload {
                success: false,
                message: Some("注册失败：必须提供有效的 task_id。".to_string()),
                assigned_client_id: client_id,
                effective_group_id: None,
                effective_role: None,
            });
        }

        // --- 步骤 1: 获取或创建组 ---
        // 使用 DashMap 的 entry API 来原子性地获取或创建组。
        // `or_try_insert_with` 在键不存在时尝试插入，如果插入的闭包返回Ok，则插入成功。
        // 这里我们需要先判断组是否存在，如果不存在则创建新组，然后对组进行写操作。
        // 如果组已存在，则直接获取写锁。

        let group_arc: Arc<RwLock<Group>>;

        if !self.groups.contains_key(&group_id) {
            info!(
                "[连接管理器::注册] 组 '{}' (任务ID: '{}') 不存在，将尝试创建新组。",
                group_id, task_id
            );
            let new_group = Group::new(group_id.clone(), task_id.clone());
            let new_group_arc = Arc::new(RwLock::new(new_group));
            
            match self.groups.insert(group_id.clone(), Arc::clone(&new_group_arc)) {
                None => { // 成功插入，这是新创建的组
                    info!(
                        "[连接管理器::注册] 新组 '{}' (任务ID: '{}') 已成功创建并添加到管理器。",
                        group_id, task_id
                    );
                    // P3.3.1: 为新创建的组初始化任务状态
                    self.task_state_manager.init_task_state(group_id.clone(), task_id.clone()).await;
                    info!(
                        "[连接管理器::注册] 已为新组 '{}' (任务ID: '{}') 调用 TaskStateManager::init_task_state。",
                        group_id, task_id
                    );
                    group_arc = new_group_arc;
                }
                Some(existing_group_arc) => { // 在我们检查和插入之间，另一个线程插入了
                    info!(
                        "[连接管理器::注册] 并发竞争：组 '{}' 在尝试插入时已被其他操作创建。使用已存在的组。",
                        group_id
                    );
                    group_arc = existing_group_arc; // 使用已存在的组
                    let existing_group_guard = group_arc.read().await;
                    if existing_group_guard.task_id != task_id {
                        warn!(
                            "[连接管理器::注册] 客户端 {} 尝试加入组 '{}'，但提供的 task_id '{}' 与组内已记录的 task_id '{}' 不匹配。注册被拒绝。",
                            client_id, group_id, task_id, existing_group_guard.task_id
                        );
                        return Err(RegisterResponsePayload {
                            success: false,
                            message: Some(format!(
                                "注册失败：提供的任务ID '{}' 与组 '{}' 已关联的任务ID '{}' 不匹配。",
                                task_id, group_id, existing_group_guard.task_id
                            )),
                            assigned_client_id: client_id,
                            effective_group_id: None,
                            effective_role: None,
                        });
                    }
                    // P3.3.1: 如果组已存在，确保任务状态也存在 (init_task_state 内部应处理重复调用)
                    self.task_state_manager.init_task_state(group_id.clone(), task_id.clone()).await;
                }
            }
        } else { // 组已存在
            group_arc = self.groups.get(&group_id).unwrap().value().clone(); // 获取已存在组的Arc
            let existing_group_guard = group_arc.read().await;
            if existing_group_guard.task_id != task_id {
                warn!(
                    "[连接管理器::注册] 客户端 {} 尝试加入组 '{}'，但提供的 task_id '{}' 与组内已记录的 task_id '{}' 不匹配。注册被拒绝。",
                    client_id, group_id, task_id, existing_group_guard.task_id
                );
                return Err(RegisterResponsePayload {
                    success: false,
                    message: Some(format!(
                        "注册失败：提供的任务ID '{}' 与组 '{}' 已关联的任务ID '{}' 不匹配。",
                        task_id, group_id, existing_group_guard.task_id
                    )),
                    assigned_client_id: client_id,
                    effective_group_id: None,
                    effective_role: None,
                });
            }
             // P3.3.1: 确保任务状态也存在 (init_task_state 内部应处理重复调用)
            self.task_state_manager.init_task_state(group_id.clone(), task_id.clone()).await;
            info!(
                "[连接管理器::注册] 组 '{}' (任务ID: '{}') 已存在。客户端 {} 尝试加入。",
                group_id, task_id, client_id
            );
        }


        // --- 步骤 2: 获取组的写锁，并检查角色冲突 ---
        let mut group = group_arc.write().await; // 获取组的异步写锁

        // 根据请求的角色检查组内是否已有同角色的客户端。
        // 同时处理将当前客户端分配到组内对应角色的槽位。
        let role_conflict_message: Option<String> = match requested_role {
            ClientRole::ControlCenter => {
                if group.control_center_client.is_some()
                    && group.control_center_client.as_ref().unwrap().client_id != client_id
                {
                    Some(format!(
                        "组 '{}' 已有一个活动的控制中心客户端。",
                        group_id
                    ))
                } else {
                    // 如果槽位为空，或者槽位上的客户端就是当前客户端 (例如，客户端断线重连并重新注册)
                    group.control_center_client = Some(Arc::clone(&client_session));
                    None // 无冲突
                }
            }
            ClientRole::OnSiteMobile => {
                if group.on_site_mobile_client.is_some()
                    && group.on_site_mobile_client.as_ref().unwrap().client_id != client_id
                {
                    Some(format!(
                        "组 '{}' 已有一个活动的现场移动端客户端。",
                        group_id
                    ))
                } else {
                    group.on_site_mobile_client = Some(Arc::clone(&client_session));
                    None // 无冲突
                }
            }
            ClientRole::Unknown => { // 不允许以 Unknown 角色注册到特定槽位
                Some("不允许以 'Unknown' 角色注册。请提供有效的客户端角色。".to_string())
            }
            // 其他角色 (如果未来支持) 在此添加类似逻辑
            // _ => Some(format!("不支持的角色: {:?}", requested_role)),
        };

        if let Some(conflict_msg) = role_conflict_message {
            warn!(
                "[连接管理器::注册] 客户端 {} 注册到组 '{}' 失败，角色冲突: {}",
                client_id, group_id, conflict_msg
            );
            // 如果是新创建的组并且注册失败（例如角色冲突），则需要考虑是否移除这个空组。
            // 但由于此时组内还没有成功加入的成员，可以暂时保留，等待其自然超时或被其他成功注册者使用。
            // 或者，如果确定是无法使用的组，可以在这里从 self.groups 中移除。
            // 当前：不立即移除组，依赖后续逻辑或超时。
            return Err(RegisterResponsePayload {
                success: false,
                message: Some(format!("注册失败：{}", conflict_msg)),
                assigned_client_id: client_id,
                effective_group_id: None, // 注册失败，没有有效组ID
                effective_role: None,     // 注册失败，没有有效角色
            });
        }

        // --- 步骤 3: 更新客户端会话自身的角色和组ID信息 ---
        // 获取客户端会话内部状态的写锁以更新其角色和组ID。
        *client_session.role.write().await = requested_role.clone();
        *client_session.group_id.write().await = Some(group_id.clone());

        info!(
            "[连接管理器::注册] 客户端 {} (角色: {:?}) 已成功加入/更新到组 '{}' (任务ID: '{}')。",
            client_id, requested_role, group_id, group.task_id // 使用 group.task_id 以确保一致性
        );
        debug!(
            "[连接管理器::注册] 组 '{}' 当前状态: 控制中心: {:?}, 现场移动端: {:?}.",
            group_id,
            group.control_center_client.as_ref().map(|cs| cs.client_id),
            group.on_site_mobile_client.as_ref().map(|cs| cs.client_id)
        );


        // --- 步骤 4: 通知组内伙伴关于当前客户端的上线状态 ---
        // (注意：此处的逻辑需要仔细处理，避免向自己发送通知，并正确识别伙伴)
        let mut partner_sessions_to_notify: Vec<Arc<ClientSession>> = Vec::new();
        
        // 根据当前客户端的角色，找到其伙伴。
        match requested_role {
            ClientRole::ControlCenter => {
                if let Some(partner) = &group.on_site_mobile_client {
                    // 确保伙伴不是自己 (理论上不太可能，因为角色不同)
                    if partner.client_id != client_id {
                        partner_sessions_to_notify.push(Arc::clone(partner));
                    }
                }
            }
            ClientRole::OnSiteMobile => {
                if let Some(partner) = &group.control_center_client {
                    if partner.client_id != client_id {
                        partner_sessions_to_notify.push(Arc::clone(partner));
                    }
                }
            }
            ClientRole::Unknown => { /* Unknown 角色不应有伙伴通知 */ }
        }

        // 向识别出的伙伴发送上线通知。
        for partner_session in partner_sessions_to_notify {
            let partner_status_payload = PartnerStatusPayload {
                partner_role: requested_role.clone(), // 上线的是当前客户端的角色
                partner_client_id: client_id,         // 上线的是当前客户端的ID
                is_online: true,                      // 状态是在线
                group_id: group_id.clone(),           // 相关的组ID
            };
            match WsMessage::new(PARTNER_STATUS_UPDATE_MESSAGE_TYPE.to_string(), &partner_status_payload) {
                Ok(ws_message) => {
                    if let Err(e) = partner_session.sender.send(ws_message).await {
                        error!(
                            "[连接管理器::注册] 向客户端 {} (伙伴 of {}) 发送伙伴上线通知失败: {}。该伙伴可能已断开。",
                            partner_session.client_id, client_id, e
                        );
                    } else {
                        info!(
                            "[连接管理器::注册] 已成功向客户端 {} (伙伴 of {}) 发送了关于客户端 {} (角色: {:?}) 上线的通知。",
                            partner_session.client_id, client_id, client_id, requested_role
                        );
                    }
                }
                Err(e) => {
                    error!(
                        "[连接管理器::注册] 创建伙伴上线通知 WsMessage 失败: {}. Payload: {:?}",
                        e, partner_status_payload
                    );
                }
            }
        }
        
        // --- 步骤 5: 通知当前客户端其伙伴（如果已存在）的在线状态 ---
        // (在释放组的写锁前完成，以保证伙伴信息的一致性)
        let mut existing_partners_for_current_client: Vec<(ClientRole, Uuid)> = Vec::new();
        match requested_role {
            ClientRole::ControlCenter => { // 当前客户端是控制中心，其伙伴是现场端
                if let Some(on_site_client) = &group.on_site_mobile_client {
                    // 确保不是自己 (虽然不太可能，因为角色不同)
                    if on_site_client.client_id != client_id {
                         existing_partners_for_current_client.push((
                            ClientRole::OnSiteMobile, // 伙伴的角色
                            on_site_client.client_id // 伙伴的ID
                        ));
                    }
                }
            }
            ClientRole::OnSiteMobile => { // 当前客户端是现场端，其伙伴是控制中心
                if let Some(control_client) = &group.control_center_client {
                    if control_client.client_id != client_id {
                        existing_partners_for_current_client.push((
                            ClientRole::ControlCenter, // 伙伴的角色
                            control_client.client_id   // 伙伴的ID
                        ));
                    }
                }
            }
            ClientRole::Unknown => { /* Unknown 角色不查找伙伴 */ }
        }

        // 为了简化，这里先收集信息，待会儿在锁外发送。

        // 临时释放组的写锁，以便可以安全地向当前客户端发送消息
        // 注意：这意味着在发送伙伴状态通知给当前客户端时，组的状态可能已经再次改变。
        // 这是一个需要权衡的设计点。如果要求严格一致性，则发送逻辑需要更复杂。
        // 当前设计：允许这种微小的时间窗口。
        drop(group); // 明确释放写锁

        for (partner_role, partner_client_id) in existing_partners_for_current_client {
             let partner_status_payload_for_self = PartnerStatusPayload {
                partner_role: partner_role.clone(), // 这是已存在伙伴的角色 - 克隆 partner_role
                partner_client_id, // 这是已存在伙伴的ID
                is_online: true, // 因为伙伴仍在组内，所以是在线
                group_id: group_id.clone(),
            };
            match WsMessage::new(PARTNER_STATUS_UPDATE_MESSAGE_TYPE.to_string(), &partner_status_payload_for_self) {
                Ok(ws_message_for_self) => {
                    if let Err(e) = client_session.sender.send(ws_message_for_self).await {
                        error!(
                            "[连接管理器::注册] 向当前客户端 {} 发送其伙伴 {} (角色 {:?}) 的在线状态失败: {}",
                            client_id, partner_client_id, partner_role, e // partner_role 在这里被借用
                        );
                    } else {
                        info!(
                            "[连接管理器::注册] 已成功向当前客户端 {} 通知其伙伴 {} (角色 {:?}) 当前在线。",
                            client_id, partner_client_id, partner_role
                        );
                    }
                }
                Err(e) => {
                     error!(
                        "[连接管理器::注册] 创建向当前客户端发送伙伴状态的 WsMessage 失败: {}. Payload: {:?}",
                        e, partner_status_payload_for_self
                    );
                }
            }
        }


        // --- 步骤 6: 返回成功的注册响应 ---
        info!(
            "[连接管理器::注册] 客户端 {} 注册流程完成。角色: {:?}, 组ID: '{}' (任务ID: '{}')",
            client_id, requested_role, group_id, task_id // 使用原始 payload 中的 task_id
        );
        Ok(RegisterResponsePayload {
            success: true,
            message: Some("成功加入组。".to_string()),
            assigned_client_id: client_id,
            effective_group_id: Some(group_id),
            effective_role: Some(requested_role),
        })
    }

    /// 获取当前所有活动客户端会话的一个快照 (克隆的 `Arc<ClientSession>` 列表)。
    /// 此方法主要用于内部监控，例如由 `HeartbeatMonitor` 定期调用以检查客户端活跃状态。
    ///
    /// # 返回值
    /// 返回一个包含所有活动 `ClientSession` 的 `Arc` 引用的 `Vec`。
    pub fn get_all_client_sessions(&self) -> Vec<Arc<ClientSession>> {
        self.clients.iter() // 迭代 DashMap 中的所有条目
            .map(|entry| entry.value().clone()) // 对每个条目，克隆其值的 Arc 引用
            .collect() // 收集结果到一个 Vec 中
    }

    /// 获取当前连接的活动客户端总数。
    /// 主要用于监控或调试。
    ///
    /// # 返回值
    /// 返回活动客户端的数量 (usize)。
    pub fn get_client_count(&self) -> usize {
        self.clients.len() // 返回 DashMap 的长度
    }
    
    /// 根据组ID获取对组信息的只读访问权。 (目前未使用，但保留用于测试或未来扩展)
    /// `#[allow(dead_code)]` 属性用于抑制编译器关于未使用代码的警告。
    #[allow(dead_code)] // 抑制未使用代码警告
    pub async fn get_group(&self, group_id: &str) -> Option<tokio::sync::OwnedRwLockReadGuard<Group>> {
        if let Some(group_entry) = self.groups.get(group_id) { // 尝试从 groups DashMap 中获取组条目
            let group_arc_rwlock = group_entry.value().clone();    // 克隆 Arc<RwLock<Group>>
            Some(group_arc_rwlock.read_owned().await) // 获取异步读锁并返回其拥有的守卫
        } else {
            None // 如果组不存在，返回 None
        }
    }
}

// 为 ConnectionManager 实现 Default trait。
// 这允许在没有明确提供 TaskStateManager 时（例如在某些测试场景或默认初始化中）创建一个实例。
impl Default for ConnectionManager {
    fn default() -> Self {
        info!("[连接管理器] 正在创建 ConnectionManager 的默认实例 (使用默认的 TaskStateManager)。");
        // 创建一个新的 TaskStateManager (使用其 default 实现) 并传递给 ConnectionManager::new
        Self::new(Arc::new(TaskStateManager::default()))
    }
} 