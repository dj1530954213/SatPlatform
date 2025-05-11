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
    PartnerStatusPayload, RegisterPayload, RegisterResponsePayload, PARTNER_STATUS_UPDATE_MESSAGE_TYPE,
    REGISTER_RESPONSE_MESSAGE_TYPE, // P3.1.2: 确保 RegisterResponse 类型被引入
};
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

                        // 根据断开连接客户端的角色，更新组内成员信息并确定其伙伴。
                        match role_at_disconnect {
                            ClientRole::ControlCenter => {
                                if let Some(cc_session) = &group.control_center_client {
                                    if cc_session.client_id == *client_id { // 确认是当前正在移除的客户端
                                        group.control_center_client = None; // 从组中移除控制中心客户端引用
                                        info!("[连接管理器::组处理] 已从组 '{}' 中移除控制中心客户端 (ID: {})。", group_id, client_id);
                                        // 其伙伴（如果存在）是现场移动端
                                        partner_session_to_notify = group.on_site_mobile_client.clone(); 
                                    } else {
                                        // 这种情况理论上不应发生，因为 client_id 是唯一的。
                                        warn!("[连接管理器::组处理] 警告：组 '{}' 中记录的控制中心客户端 (ID: {}) 与正在移除的客户端 (ID: {}) 不匹配。未从组中移除。", 
                                            group_id, cc_session.client_id, client_id);
                                    }
                                } else {
                                    // 客户端角色是控制中心，但组内并未记录它。这可能表示状态不一致或之前的移除操作已部分完成。
                                    info!("[连接管理器::组处理] 客户端 {} (角色: 控制中心) 断开时，组 '{}' 中并未记录其为控制中心成员。", client_id, group_id);
                                }
                            }
                            ClientRole::OnSiteMobile => {
                                if let Some(os_session) = &group.on_site_mobile_client {
                                    if os_session.client_id == *client_id { // 确认是当前客户端
                                        group.on_site_mobile_client = None; // 从组中移除现场移动端客户端引用
                                        info!("[连接管理器::组处理] 已从组 '{}' 中移除现场移动端客户端 (ID: {})。", group_id, client_id);
                                        // 其伙伴（如果存在）是控制中心
                                        partner_session_to_notify = group.control_center_client.clone(); 
                                    } else {
                                        warn!("[连接管理器::组处理] 警告：组 '{}' 中记录的现场移动端客户端 (ID: {}) 与正在移除的客户端 (ID: {}) 不匹配。未从组中移除。", 
                                            group_id, os_session.client_id, client_id);
                                    }
                                } else {
                                    info!("[连接管理器::组处理] 客户端 {} (角色: 现场移动端) 断开时，组 '{}' 中并未记录其为现场移动端成员。", client_id, group_id);
                                }
                            }
                            ClientRole::Unknown => {
                                // 此情况理论上已被外部的 if role_at_disconnect == ClientRole::Unknown 检查覆盖，不应到达此处。
                                // 但作为防御性编程，记录一个警告。
                                warn!("[连接管理器::组处理] 内部逻辑错误：客户端 {} 在处理组内移除时角色仍为未知，这不应发生。", client_id);
                            }
                        }

                        // 如果存在伙伴，则需要异步通知该伙伴当前客户端已下线。
                        if let Some(partner_session) = partner_session_to_notify { // 使用新的变量名以示清晰
                            info!(
                                "[连接管理器::伙伴通知] 组 '{}' 中存在伙伴 (ID: {}, 角色: {:?})。正在准备通知其关于客户端 {} (角色: {:?}) 的下线状态。",
                                group_id, partner_session.client_id, *partner_session.role.read().await, client_id, role_at_disconnect
                            );
                            // 构建伙伴状态更新负载 (PartnerStatusPayload)
                            let status_payload = PartnerStatusPayload {
                                partner_role: role_at_disconnect.clone(), // 下线伙伴的角色
                                partner_client_id: *client_id,            // 下线伙伴的客户端ID
                                is_online: false,                         // 状态明确为下线
                                group_id: group_id.clone(),               // 所属组ID，payload 中包含以供接收方确认上下文
                            };

                            // 为 tokio::spawn 准备所需数据的拥有权副本 (Arc 的克隆是廉价的，仅增加引用计数)。
                            let client_id_for_log = *client_id;
                            let group_id_for_log = group_id.clone();
                            let partner_session_for_spawn = partner_session.clone(); 

                            // 异步派生一个新任务来发送通知，以避免阻塞当前的 remove_client 方法。
                            // 这对于保持 ConnectionManager 的响应性很重要。
                            tokio::spawn(async move {
                                // 在此异步块内部，使用克隆的/拥有的数据。
                                match serde_json::to_string(&status_payload) { // status_payload 内部的 group_id 已是克隆的
                                    Ok(json_payload) => {
                                        match WsMessage::new(
                                            PARTNER_STATUS_UPDATE_MESSAGE_TYPE.to_string(), // 使用预定义的常量作为消息类型
                                            &json_payload, // 传入序列化后的 JSON 字符串作为负载
                                        ) {
                                            Ok(ws_msg) => {
                                                // 尝试通过伙伴会话的 sender 将消息发送到其出站队列。
                                                if let Err(e) = partner_session_for_spawn.sender.send(ws_msg).await {
                                                    error!(
                                                        "[连接管理器::伙伴通知任务] 向组 '{}' 中的伙伴 {} (地址: {}) 发送伙伴状态更新 ({} 已下线) 失败: {}",
                                                        group_id_for_log, partner_session_for_spawn.client_id, partner_session_for_spawn.addr, client_id_for_log, e 
                                                    );
                                                } else {
                                                    info!(
                                                        "[连接管理器::伙伴通知任务] 已成功向伙伴 {} (地址: {}) 发送关于客户端 {} 的伙伴状态更新 (下线)。组ID: '{}'",
                                                        partner_session_for_spawn.client_id, partner_session_for_spawn.addr, client_id_for_log, group_id_for_log 
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                error!("[连接管理器::伙伴通知任务] 为组 '{}' 创建伙伴状态更新 (下线) WsMessage 失败 (序列化后的 Payload: '{}'): {}", group_id_for_log, json_payload, e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        // 这种情况通常不应发生，除非 PartnerStatusPayload 的定义与 serde 实现存在问题。
                                        error!("[连接管理器::伙伴通知任务] 为组 '{}' 序列化 PartnerStatusPayload (下线) 失败: {}. Payload详情: {:?}", group_id_for_log, e, status_payload);
                                    }
                                }
                            });
                        } else {
                            info!("[连接管理器::伙伴通知] 客户端 {} (角色: {:?}) 在组 '{}' 中断开时，组内无其他伙伴。无需发送下线通知。", client_id, role_at_disconnect, group_id);
                        }

                        // 检查组在移除了当前客户端后是否变为空 (即没有任何成员了)。
                        if group.control_center_client.is_none() && group.on_site_mobile_client.is_none() {
                            info!(
                                "[连接管理器::组清理] 客户端 {} (角色: {:?}) 断开后，组 '{}' (任务ID: '{}') 已变为空。正在从管理器中移除此组并通知任务状态管理器。",
                                client_id, role_at_disconnect, group_id, group.task_id
                            );
                            // 在从 `self.groups` 映射中实际移除组条目之前，需要获取 task_id 并显式 drop `group` 的写锁，
                            // 以避免在持有锁的情况下调用可能也需要锁的 `self.groups.remove` 或 `self.task_state_manager` 方法。
                            let task_id_for_removal = group.task_id.clone(); // 克隆 task_id 以备后续使用
                            let group_id_for_removal = group.group_id.clone(); // 克隆 group_id 同样用于后续
                            drop(group); // 显式释放对 group 的写锁，此时 group_lock (Arc<RwLock<Group>>) 仍然存在于 DashMap 中

                            // 现在可以安全地从 `self.groups` 映射中移除这个空组的条目了。
                            if self.groups.remove(&group_id_for_removal).is_some() {
                                info!("[连接管理器::组清理] 空组 '{}' 已成功从 ConnectionManager 的活动组列表中移除。", group_id_for_removal);
                            } else {
                                // 这可能表示在释放锁到尝试移除之间，该组已被其他操作移除了，理论上不太可能但作为防御性日志。
                                warn!("[连接管理器::组清理] 尝试移除空组 '{}' 时，它已不在 ConnectionManager 的活动组列表中。可能已被并发移除。", group_id_for_removal);
                            }

                            // 通知 TaskStateManager 清理与此已解散的组关联的任务状态。
                            // 同样，这是一个异步操作，我们派生一个新任务来执行它。
                            let task_state_manager_clone = self.task_state_manager.clone();
                            tokio::spawn(async move {
                                info!(
                                    "[连接管理器::任务状态清理任务] 正在为已解散的组 '{}' (原任务ID: '{}') 调用 TaskStateManager 清理任务状态...",
                                    group_id_for_removal, task_id_for_removal
                                );
                                task_state_manager_clone.remove_task_state(&group_id_for_removal, &task_id_for_removal).await;
                                // remove_task_state 内部应有自己的日志记录其操作结果。
                            });

                        } else {
                            info!(
                                "[连接管理器::组处理] 客户端 {} (角色: {:?}) 断开后，组 '{}' (任务ID: '{}') 中仍有其他成员。组保持活动状态。控制中心: {:?}, 现场端: {:?}",
                                client_id, role_at_disconnect, group.group_id, group.task_id, 
                                group.control_center_client.as_ref().map(|s| s.client_id),
                                group.on_site_mobile_client.as_ref().map(|s| s.client_id)
                            );
                        }
                    } else {
                        // 客户端声称属于一个组，但该组ID在 ConnectionManager 的 groups 映射中找不到。
                        // 这可能是一个状态不一致的信号，或者客户端断开时组已被其他原因移除。
                        warn!(
                            "[连接管理器] 客户端 {} (角色: {:?}) 声称属于组 '{}'，但在尝试处理其移除时，该组在 ConnectionManager 中未找到。可能已被并发移除。", 
                            client_id, role_at_disconnect, group_id
                        );
                    }
                }
            } else {
                // 客户端在断开时 `group_id` 为 `None`，表示它从未成功加入任何组，或者已被正确地从组中移除。
                info!(
                    "[连接管理器] 客户端 {} (角色: {:?}) 在断开时未属于任何组。无需进行组相关的清理操作。",
                    client_id, role_at_disconnect
                );
            }
            info!("[连接管理器] 客户端 {} 的移除处理已完成。当前活动客户端总数: {}", client_id, self.clients.len());

        } else {
            // 如果 `self.clients.remove(client_id)` 返回 `None`，表示指定的 `client_id` 不在活动客户端列表中。
            // 这可能是因为该客户端已被其他并发操作（例如另一次超时检测或并发的断开事件）移除了，或者提供的 `client_id` 无效。
            warn!(
                "[连接管理器] 尝试移除客户端 {} 时，该客户端未在活动客户端列表中找到。可能已被并发移除或ID无效。",
                client_id
            );
        }
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
        client_session: Arc<ClientSession>>,
        payload: RegisterPayload,
    ) -> Result<RegisterResponsePayload, RegisterResponsePayload> {
        let client_id = client_session.client_id;
        let requested_group_id = payload.group_id.clone();
        let requested_role = payload.role.clone();
        let task_id_from_payload = payload.task_id.clone();

        info!(
            "[连接管理器::入组] 客户端 {} (地址: {}) 请求加入组: '{}', 声明角色: {:?}, 关联任务ID: '{}'",
            client_id, client_session.addr, requested_group_id, requested_role, task_id_from_payload
        );

        // 检查客户端是否已有所属组。如果已有，则不允许重复加入，除非是重新加入同一个组（此逻辑暂不处理，视为错误）。
        // （此检查也可以放在更早，例如 MessageRouter，但放在这里更接近状态修改）
        let current_group_lock = client_session.group_id.read().await;
        if let Some(existing_group_id) = &*current_group_lock {
            if *existing_group_id == requested_group_id {
                 warn!(
                    "[连接管理器::入组] 客户端 {} 已在组 '{}' 中，尝试重复加入同一组。将视为错误处理。", 
                    client_id, existing_group_id
                );
                // 可以选择允许重新加入同一组并更新角色等，但当前简化为错误。
            } else {
                warn!(
                    "[连接管理器::入组] 客户端 {} 已属于组 '{}'，尝试加入新组 '{}' (角色 {:?}) 失败。客户端需先离开原组。",
                    client_id, existing_group_id, requested_group_id, requested_role
                );
            }
            return Err(RegisterResponsePayload {
                success: false,
                message: Some(format!("客户端已在组 '{}' 中，无法加入新组或重复加入。", existing_group_id)),
                assigned_client_id: client_id,
                effective_group_id: Some(existing_group_id.clone()),
                effective_role: Some(client_session.role.read().await.clone()), // 返回其当前已分配的角色
            });
        }
        drop(current_group_lock); // 释放 group_id 读锁

        // 获取或创建组的可写锁
        // DashMap的 `entry` API 提供了原子性的获取或插入操作，非常适合这种场景。
        // `or_try_insert_with` 在键不存在时尝试插入，存在时返回值。
        // 这里我们使用更直接的方式：先检查，再获取或创建，以允许更细致的日志和错误处理。

        let group_entry = self.groups.entry(requested_group_id.clone()).or_insert_with(|| {
            info!(
                "[连接管理器::入组] 组 '{}' 不存在，将为其创建新组并关联任务ID '{}'。",
                requested_group_id, task_id_from_payload
            );
            // 当组是新创建时，调用 task_state_manager 初始化任务状态
            // 注意：这里是在 or_insert_with 闭包内，不是异步上下文，不能直接 .await
            // 因此，TaskStateManager 的 init_task_state 调用需要移到获取组锁之后，或者 TaskStateManager 提供同步接口（不推荐）
            // 或者在这里仅记录需要初始化，在后续获取锁后执行。 P3.1.2 原设计是在 join_group 内部异步调用。
            // 修正：TaskStateManager 初始化应在获得组的写锁之后，且确认客户端可以加入时进行。
            Arc::new(RwLock::new(Group::new(requested_group_id.clone(), task_id_from_payload.clone())))
        });
        
        let group_arc_rwlock = group_entry.value().clone(); // Arc<RwLock<Group>>
        let mut group = group_arc_rwlock.write().await; // 获取组的异步写锁

        // 在获得组的写锁后，再次确认 task_id 是否一致（如果组已存在）
        // 这是为了处理一种边缘情况：两个客户端几乎同时尝试用相同的 group_id 但不同的 task_id 创建组。
        // DashMap 的 entry().or_insert_with() 能保证原子性地创建一次组，
        // 但如果 task_id 在 Payload 中是可变的，且策略是组一旦创建其 task_id 不可更改，则需要此检查。
        if group.task_id != task_id_from_payload {
            warn!(
                "[连接管理器::入组] 客户端 {} 尝试加入组 '{}' (任务ID '{}')，但该组已存在且关联了不同的任务ID '{}'。加入失败。", 
                client_id, requested_group_id, task_id_from_payload, group.task_id
            );
            // 释放写锁，因为我们没有修改组，只是读取了 task_id
            drop(group);
            return Err(RegisterResponsePayload {
                success: false,
                message: Some(format!("组 '{}' 已存在并关联到不同的任务 (ID: {}).", requested_group_id, group.task_id)),
                assigned_client_id: client_id,
                effective_group_id: None, 
                effective_role: None,
            });
        }

        // 角色冲突检查及成员分配
        let mut partner_session_for_notification: Option<Arc<ClientSession>> = None;
        let mut registration_successful = false;

        match requested_role {
            ClientRole::ControlCenter => {
                if group.control_center_client.is_none() {
                    // 组内尚无控制中心，可以加入
                    group.control_center_client = Some(client_session.clone());
                    partner_session_for_notification = group.on_site_mobile_client.clone(); // 其伙伴是现场端 (如果存在)
                    registration_successful = true;
                    info!(
                        "[连接管理器::入组] 客户端 {} (地址: {}) 已作为 控制中心 成功加入组 '{}' (任务ID: '{}')。",
                        client_id, client_session.addr, requested_group_id, group.task_id
                    );
                } else if let Some(existing_cc) = &group.control_center_client {
                    if existing_cc.client_id == client_id {
                        // 同一个客户端重复注册到同一角色，可以视为成功或更新（当前简化为允许，但无特殊处理）
                        partner_session_for_notification = group.on_site_mobile_client.clone();
                        registration_successful = true;
                        info!(
                            "[连接管理器::入组] 客户端 {} (控制中心) 重复注册到组 '{}' 的同一角色。视为成功。",
                            client_id, requested_group_id
                        );
                    } else {
                        // 组内已有其他控制中心
                        warn!(
                            "[连接管理器::入组] 客户端 {} 尝试作为 控制中心 加入组 '{}' 失败：该组已存在控制中心 (ID: {}).",
                            client_id, requested_group_id, existing_cc.client_id
                        );
                    }
                } // else 分支已由 is_none() 覆盖
            }
            ClientRole::OnSiteMobile => {
                if group.on_site_mobile_client.is_none() {
                    // 组内尚无现场移动端，可以加入
                    group.on_site_mobile_client = Some(client_session.clone());
                    partner_session_for_notification = group.control_center_client.clone(); // 其伙伴是控制中心 (如果存在)
                    registration_successful = true;
                    info!(
                        "[连接管理器::入组] 客户端 {} (地址: {}) 已作为 现场移动端 成功加入组 '{}' (任务ID: '{}')。",
                        client_id, client_session.addr, requested_group_id, group.task_id
                    );
                } else if let Some(existing_osm) = &group.on_site_mobile_client {
                    if existing_osm.client_id == client_id {
                        // 同一个客户端重复注册到同一角色
                        partner_session_for_notification = group.control_center_client.clone();
                        registration_successful = true;
                        info!(
                            "[连接管理器::入组] 客户端 {} (现场移动端) 重复注册到组 '{}' 的同一角色。视为成功。",
                            client_id, requested_group_id
                        );
                    } else {
                        // 组内已有其他现场移动端
                        warn!(
                            "[连接管理器::入组] 客户端 {} 尝试作为 现场移动端 加入组 '{}' 失败：该组已存在现场移动端 (ID: {}).",
                            client_id, requested_group_id, existing_osm.client_id
                        );
                    }
                }
            }
            ClientRole::Unknown => {
                // 不允许以 Unknown 角色注册到组
                warn!(
                    "[连接管理器::入组] 客户端 {} 尝试以 未知(Unknown) 角色加入组 '{}'。操作被拒绝。",
                    client_id, requested_group_id
                );
            }
        }

        if registration_successful {
            // 更新客户端会话自身的状态 (role 和 group_id)
            *client_session.role.write().await = requested_role.clone();
            *client_session.group_id.write().await = Some(requested_group_id.clone());
            info!(
                "[连接管理器::入组] 客户端 {} 的会话状态已更新：角色 -> {:?}, 组ID -> '{}'",
                client_id, requested_role, requested_group_id
            );

            // P3.1.2 & P3.3.1: 调用 TaskStateManager 初始化任务状态。
            // 这个调用应该在确认客户端可以成功加入组，并且获得了组的task_id之后。
            // 之前 or_insert_with 中的注释提到此问题，这里是正确的执行点。
            // 检查组是否是"新"的，或者这是否是第一个"有意义"的客户端加入。 
            // 一个简单的判断是，如果 TaskStateManager 尚未包含此 group_id 的状态，则初始化。
            // TaskStateManager::init_task_state 内部应能处理重复调用（幂等性）。
            let task_id_for_init = group.task_id.clone(); // 使用组内权威的 task_id
            let group_id_for_init = group.group_id.clone();
            let task_state_manager_clone = self.task_state_manager.clone();
            tokio::spawn(async move {
                info!(
                    "[连接管理器::任务状态初始化任务] 正在为组 '{}' (任务ID: '{}') 调用 TaskStateManager 初始化任务状态...",
                    group_id_for_init, task_id_for_init
                );
                task_state_manager_clone.init_task_state(group_id_for_init, task_id_for_init).await;
            });


            // P3.1.3: 通知逻辑
            // 1. 通知新加入的客户端其伙伴的状态 (如果伙伴存在)
            if let Some(partner_session) = &partner_session_for_notification {
                let partner_role_for_newcomer = partner_session.role.read().await.clone();
                let partner_id_for_newcomer = partner_session.client_id;
                info!(
                    "[连接管理器::伙伴通知] 准备通知新加入的客户端 {} (角色 {:?}) 关于其伙伴 {} (角色 {:?}) 的在线状态。",
                    client_id, requested_role, partner_id_for_newcomer, partner_role_for_newcomer
                );
                let payload_to_newcomer = PartnerStatusPayload {
                    partner_role: partner_role_for_newcomer.clone(),
                    partner_client_id: partner_id_for_newcomer,
                    is_online: true, // 伙伴是在线的
                    group_id: requested_group_id.clone(),
                };
                let newcomer_session_clone = client_session.clone();
                let group_id_log_clone = requested_group_id.clone();
                tokio::spawn(async move {
                    match serde_json::to_string(&payload_to_newcomer) {
                        Ok(json_payload) => {
                            match WsMessage::new(PARTNER_STATUS_UPDATE_MESSAGE_TYPE.to_string(), &json_payload) {
                                Ok(ws_msg) => {
                                    if let Err(e) = newcomer_session_clone.sender.send(ws_msg).await {
                                        error!(
                                            "[连接管理器::伙伴通知任务] 向新客户端 {} (组 '{}') 发送其伙伴在线状态失败: {}",
                                            newcomer_session_clone.client_id, group_id_log_clone, e
                                        );
                                    } else {
                                        info!("[连接管理器::伙伴通知任务] 已成功向新客户端 {} (组 '{}') 发送其伙伴 {} (角色 {:?}) 的在线状态。", 
                                            newcomer_session_clone.client_id, group_id_log_clone, payload_to_newcomer.partner_client_id, payload_to_newcomer.partner_role);
                                    }
                                }
                                Err(e) => error!("[连接管理器::伙伴通知任务] 为新客户端 {} (组 '{}') 创建伙伴状态消息 (通知其伙伴在线) 失败: {}", newcomer_session_clone.client_id, group_id_log_clone, e),
                            }
                        }
                        Err(e) => error!("[连接管理器::伙伴通知任务] 为新客户端 {} (组 '{}') 序列化伙伴状态负载 (通知其伙伴在线) 失败: {}", newcomer_session_clone.client_id, group_id_log_clone, e),
                    }
                });
            } else {
                 info!(
                    "[连接管理器::伙伴通知] 新加入的客户端 {} (角色 {:?}) 在组 '{}' 中尚无伙伴。",
                    client_id, requested_role, requested_group_id
                );
            }

            // 2. 通知已存在的伙伴关于新客户端的上线状态 (如果伙伴存在)
            if let Some(partner_session) = partner_session_for_notification { // partner_session_for_notification 指向的是 *已存在的* 伙伴
                 info!(
                    "[连接管理器::伙伴通知] 准备通知已存在的伙伴 {} (角色 {:?}) 关于新客户端 {} (角色 {:?}) 的上线状态。",
                    partner_session.client_id, *partner_session.role.read().await, client_id, requested_role
                );
                let payload_to_existing_partner = PartnerStatusPayload {
                    partner_role: requested_role.clone(),             // 新加入的客户端的角色
                    partner_client_id: client_id,                     // 新加入的客户端的ID
                    is_online: true,                                  // 状态为上线
                    group_id: requested_group_id.clone(),
                };
                // let existing_partner_session_clone = partner_session.clone(); // partner_session 本身就是 Arc，可直接用
                let group_id_log_clone_for_partner = requested_group_id.clone();
                tokio::spawn(async move {
                    match serde_json::to_string(&payload_to_existing_partner) {
                        Ok(json_payload) => {
                            match WsMessage::new(PARTNER_STATUS_UPDATE_MESSAGE_TYPE.to_string(), &json_payload) {
                                Ok(ws_msg) => {
                                    if let Err(e) = partner_session.sender.send(ws_msg).await { // 直接用 partner_session
                                        error!(
                                            "[连接管理器::伙伴通知任务] 向已存在伙伴 {} (组 '{}') 发送新成员 {} (角色 {:?}) 上线状态失败: {}",
                                            partner_session.client_id, group_id_log_clone_for_partner, payload_to_existing_partner.partner_client_id, payload_to_existing_partner.partner_role, e
                                        );
                                    } else {
                                        info!("[连接管理器::伙伴通知任务] 已成功向已存在伙伴 {} (组 '{}') 发送新成员 {} (角色 {:?}) 的上线状态。",
                                            partner_session.client_id, group_id_log_clone_for_partner, payload_to_existing_partner.partner_client_id, payload_to_existing_partner.partner_role);
                                    }
                                }
                                Err(e) => error!("[连接管理器::伙伴通知任务] 为已存在伙伴 {} (组 '{}') 创建新成员上线消息失败: {}", partner_session.client_id, group_id_log_clone_for_partner, e),
                            }
                        }
                        Err(e) => error!("[连接管理器::伙伴通知任务] 为已存在伙伴 {} (组 '{}') 序列化新成员上线负载失败: {}", partner_session.client_id, group_id_log_clone_for_partner, e),
                    }
                });
            }
            // 释放组的写锁，因为所有修改已完成
            drop(group);

            // 返回成功的响应负载
            Ok(RegisterResponsePayload {
                success: true,
                message: Some(format!("已作为 {:?} 成功加入组 '{}'。", requested_role, requested_group_id)),
                assigned_client_id: client_id,
                effective_group_id: Some(requested_group_id),
                effective_role: Some(requested_role),
            })
        } else {
            // 注册不成功 (例如角色冲突或尝试以 Unknown 角色加入)
            // 此时 group 的写锁仍然被持有，需要释放
            let error_message = match requested_role {
                ClientRole::Unknown => "不允许以未知角色加入组。".to_string(),
                _ => format!("角色 {:?} 在组 '{}' 中已存在或不允许加入。", requested_role, requested_group_id),
            };
            drop(group); // 释放写锁

            Err(RegisterResponsePayload {
                success: false,
                message: Some(error_message),
                assigned_client_id: client_id,
                effective_group_id: None,
                effective_role: None, 
            })
        }
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