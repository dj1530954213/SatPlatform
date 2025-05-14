//! 任务状态管理器模块。
//!
//! 本模块的核心职责是集中管理和维护所有活动调试任务的权威状态数据。
//! 服务端的 `TaskStateManager` (任务状态管理器) 持有每个进行中调试任务的完整、最新的状态模型
//! (例如，一个 `TaskDebugState` (任务调试状态) 结构体实例，其中包含了预检项列表、测试步骤、
//! 联锁条件等所有相关信息及其当前状态)。
//!
//! (重要提示：P3.1.2 开发阶段，本模块 (`task_state_manager.rs`) 仅为一个骨架 (skeleton) 实现。
//!  它主要定义了 `TaskStateManager` (任务状态管理器) 结构体和一些核心方法的基本签名，
//!  但这些方法内部的逻辑当前为空，或者仅包含日志记录语句。详细和完整的状态管理功能，
//!  包括引入并发安全的数据结构来实际存储任务状态 (例如，使用 `DashMap` 存储多个 `TaskDebugState` (任务调试状态) 实例)，
//!  以及实现完整的状态创建、更新、查询和清理逻辑，将在 P3.3.1 及后续的开发阶段进行详细设计和编码实现。)
//!
//! # 主要功能 (P3.3.1 及后续阶段的规划)
//! - **状态存储**: 使用高效且线程安全的数据结构 (例如，一个 `DashMap<GroupIdString, Arc<RwLock<TaskDebugState>>>`)
//!   来存储每个活动调试任务 (通常以其唯一的 `GroupIdString` (组ID字符串) 作为键) 的详细状态 (`TaskDebugState` - 任务调试状态)。
//!   这里的 `Arc<RwLock<...>>` 结构用于确保对单个 `TaskDebugState` (任务调试状态) 的并发访问既安全又高效。
//! - **状态初始化**: 当一个新的调试任务组通过 `ConnectionManager::join_group` (连接管理器的加入组方法) 成功创建后，
//!   应调用本管理器的 `init_task_state` (初始化任务状态) 方法。此方法将负责为这个新组关联的调试任务加载其初始的 `TaskDebugState` (任务调试状态)
//!   (可能从数据库、配置文件或其他数据源获取任务模板)，或创建一个全新的默认状态实例。
//! - **状态更新**: 提供一系列方法 (例如，一个通用的 `update_task_debug_state` (更新任务调试状态) 方法，或针对特定操作的更具体方法)，
//!   允许 `MessageRouter` (消息路由器) (在处理来自客户端的、与特定业务逻辑相关的 WebSocket 消息时)
//!   或其他内部服务来安全地修改某个特定任务的状态。这些更新方法必须确保操作的原子性和数据的一致性。
//! - **状态查询与快照**: 提供方法 (例如 `get_task_state_snapshot` - 获取任务状态快照) 来获取某个任务当前状态的一个只读副本或快照。
//!   这对于向客户端同步完整的任务状态 (例如，当客户端刚加入一个已在进行的任务组时)，或供其他服务端模块参考任务数据时非常有用。
//! - **状态清理**: 当一个调试任务正常结束，或其相关的客户端组被解散 (例如，所有客户端都已断开连接) 时，
//!   本管理器需要负责从其内部存储中移除对应的任务状态，以释放占用的系统资源。

use log::{info, warn, error}; // 移除了未使用的 'debug' 和 'error'
use std::sync::Arc; // `Arc` (原子引用计数) 将用于安全地共享 TaskStateManager 实例以及单个 TaskDebugState 实例的所有权。
use dashmap::DashMap; // `DashMap` 是一个高性能的并发哈希映射库，计划用于存储 `group_id` 到 `TaskDebugState` 的映射。
use tokio::sync::RwLock; // Tokio 提供的异步读写锁 (`RwLock`)，将用于保护对单个 `TaskDebugState` 实例内部数据的并发读写访问，确保数据一致性。
use common_models::TaskDebugState; // 从 `common_models` (公共模型) crate 引入任务调试状态的详细结构体定义。
use common_models::enums::ClientRole; // 从 `common_models` (公共模型) crate 引入客户端角色枚举，在实现 P3.3.1 的状态更新方法时可能会根据角色进行权限检查或逻辑分支。
use common_models::ws_payloads::BusinessActionPayload; // 引入业务Action Payload
use common_models::task_models::PreCheckItemStatus; // Keep PreCheckItemStatus
use chrono::Utc; // 引入Utc以获取当前时间
use serde_json; // serde_json is used
use uuid; // uuid is used

/// `TaskStateManager` (任务状态管理器) 结构体的定义 (当前为P3.1.2阶段的骨架实现)。
/// 
/// 在 P3.3.1 及后续的完整功能实现阶段，此结构体将包含一个核心字段，用于存储和管理多个活动调试任务的
/// `TaskDebugState` (任务调试状态) 实例。这个字段通常会是一个被 `Arc` (原子引用计数) 包裹的并发安全集合，
/// 例如 `Arc<DashMap<String, Arc<RwLock<TaskDebugState>>>>`。
/// 具体解释如下：
/// - 外层的 `Arc` 使得 `TaskStateManager` (任务状态管理器) 实例本身可以被安全地共享给多个需要访问它的地方
///   (例如，可以作为 Tauri 应用的托管状态 (`tauri::State<TaskStateManager>`)，或者被其他服务持有其共享引用)。
/// - `DashMap<String, ...>` 提供了一个高性能的、线程安全的哈希映射实现。它的键 (Key) 通常是 `group_id` (字符串类型)，
///   这个 `group_id` 唯一地标识了一个正在进行的调试任务会话或客户端组。
/// - `DashMap` 的值 (Value) 是 `Arc<RwLock<TaskDebugState>>`：
///   - 这里的内层 `Arc` 允许对单个任务的 `TaskDebugState` (任务调试状态) 的引用被安全地克隆并分发给多个
///     可能并发处理此任务状态的不同代码路径 (例如，多个处理同一任务组消息的请求)。
///   - `RwLock<TaskDebugState>` (异步读写锁) 则用于保护 `TaskDebugState` (任务调试状态) 结构体内部的实际数据。
///     它允许多个并发的读取者，但在有写入者时会确保独占访问，从而保证了任务状态数据在并发修改时的
///     一致性和完整性。
#[derive(Debug, Clone)] // 自动派生 `Debug` trait，使得 `TaskStateManager` 实例可以使用 `{:?}` 格式化操作符进行打印，便于调试。
                         // 自动派生 `Clone` trait。对于当前的骨架实现 (没有内部复杂字段)，这个 `Clone` 是简单的。
                         // 在 P3.3.1 的完整实现中，如果 `TaskStateManager` 内部持有一个如 `active_task_states: Arc<DashMap<...>>` 这样的字段，
                         // 那么 `TaskStateManager::clone()` 操作实际上只会克隆这个 `Arc` (原子引用计数指针)，
                         // 这是一种浅拷贝，只增加引用计数，而不会复制底层的 `DashMap` 数据。这正是共享状态管理所期望的行为。
pub struct TaskStateManager {
    /// 存储活动任务状态的线程安全哈希映射。
    /// 键: `group_id` (String)
    /// 值: `Arc<RwLock<TaskDebugState>>` - 对任务状态的线程安全引用，允许并发读写。
    active_task_states: DashMap<String, Arc<RwLock<TaskDebugState>>>,
}

impl TaskStateManager {
    /// 创建一个新的 `TaskStateManager` (任务状态管理器) 实例 (当前为P3.1.2阶段的骨架实现)。
    /// 
    /// 在 P3.3.1 阶段的完整功能实现中，此构造函数将负责正确初始化其内部用于存储
    /// 所有活动任务状态的并发数据结构。例如，它会创建一个新的 `DashMap` 实例，
    /// 并用 `Arc` (原子引用计数) 将其包装起来，然后存储在 `active_task_states` 字段中。
    ///
    /// # 返回值
    /// 返回一个新创建的 `TaskStateManager` (任务状态管理器) 实例 (在当前骨架实现中，其内部状态为空)。
    pub fn new() -> Self {
        info!("[任务状态管理器] 正在创建一个新的 TaskStateManager (任务状态管理器) 实例。(当前为P3.1.2骨架实现，内部实际状态存储尚未初始化)。");
        Self {
            active_task_states: DashMap::new(),
        }
    }

    /// 初始化或关联特定调试任务的状态 (当前为P3.1.2阶段的骨架实现，主要功能是记录日志)。
    /// 
    /// 在 P3.3.1 阶段的完整功能实现中，此异步方法 (`async fn`) 将承担以下关键职责：
    /// 1.  接收一个 `group_id` (组ID，字符串类型) 和一个 `task_id` (任务ID，字符串类型)。
    ///     `group_id` 通常由 `ConnectionManager` (连接管理器) 在客户端成功加入一个调试组后提供，
    ///     而 `task_id` 则指定了要初始化的具体任务的类型或标识，可能用于从数据源加载任务模板。
    /// 2.  检查内部的 `active_task_states` (活动任务状态) 集合 (在P3.3.1中会是 `DashMap`) 
    ///     中是否已经存在以传入的 `group_id` 为键的任务状态条目。
    /// 3.  如果该 `group_id` 对应的任务状态尚不存在：
    ///     a.  则需要根据 `task_id` 来创建或加载一个全新的 `TaskDebugState` (任务调试状态) 实例。
    ///         这可能涉及到从数据库查询任务模板、从配置文件加载默认设置，或基于 `task_id` 应用某种工厂模式来构造初始状态。
    ///     b.  将这个新创建的 `TaskDebugState` (任务调试状态) 实例用 `RwLock` (读写锁) 和 `Arc` (原子引用计数) 包装起来，
    ///         即 `Arc<RwLock<TaskDebugState>>`。
    ///     c.  然后将这个包装后的共享状态引用插入到 `active_task_states` (活动任务状态) 集合中，
    ///         使用 `group_id` 作为其键。
    /// 4.  如果 `group_id` 对应的任务状态已存在，则根据具体的设计决策，可能执行重新初始化逻辑 (如果允许)，
    ///     或者简单地记录一个警告并返回 (如果一个组的任务状态一旦初始化后不应被覆盖)。
    /// 5.  在整个过程中，记录详细的日志信息，包括操作的参数、执行的步骤以及结果状态。
    ///
    /// # 参数
    /// * `group_id`: `String` - 唯一标识一个客户端调试任务组或会话的字符串。此 ID 将被用作在内部
    ///   `active_task_states` (活动任务状态) 集合中存储和检索该任务对应状态的键。
    /// * `task_id`: `String` - 需要为其初始化状态的原始调试任务的唯一标识符。这个 ID 可能会被用于
    ///   从某个外部数据源 (如数据库、配置文件) 加载该任务的初始配置、模板数据或默认设置。
    pub async fn init_task_state(&self, group_id: String, task_id: String) {
        if self.active_task_states.contains_key(&group_id) {
            warn!(
                "尝试为已存在的 group_id '{}' 初始化任务状态 (task_id: '{}')。可能是一个重复调用。",
                group_id, task_id
            );
            // 根据需求，这里可以决定是否需要更新 task_id (如果允许改变的话)，或直接返回。
            // 当前实现为简单忽略重复初始化，不覆盖已存在的状态。
            return;
        }

        info!("为 group_id '{}' 初始化任务状态，关联 task_id '{}'", group_id, task_id);
        let new_task_state = TaskDebugState::new(task_id.clone());
        self.active_task_states.insert(group_id.clone(), Arc::new(RwLock::new(new_task_state)));
        info!("任务状态 (task_id: '{}') 已成功为 group_id '{}' 创建并存储。", task_id, group_id);
    }

    /// 根据 `group_id` 获取对任务状态的只读访问权（如果存在）。
    ///
    /// # Arguments
    /// * `group_id` - 要查询的任务状态所属的组ID。
    ///
    /// # Returns
    /// * `Some(Arc<RwLock<TaskDebugState>>)` 如果找到了对应组的任务状态。
    /// * `None` 如果没有找到。
    pub async fn get_task_state(&self, group_id: &str) -> Option<Arc<RwLock<TaskDebugState>>> {
        // DashMap 的 get 方法返回的是一个 Ref<K, V>，我们需要克隆 Arc<RwLock<TaskDebugState>>
        // DashMap 的 get 方法返回一个 Guard 对象，该对象在作用域结束时释放读锁。
        // 为了将 Arc<RwLock<TaskDebugState>> 返回并让调用者持有它，我们需要克隆这个 Arc。
        match self.active_task_states.get(group_id) {
            Some(task_state_ref) => {
                info!("为 group_id '{}' 获取任务状态的只读访问权。", group_id);
                Some(task_state_ref.value().clone()) // 克隆 Arc
            }
            None => {
                warn!("尝试获取不存在的 group_id '{}' 的任务状态。", group_id);
                None
            }
        }
    }

    /// 此方法从内部的 `active_task_states` 集合中异步移除与指定 `group_id` 关联的 `TaskDebugState`。
    /// 如果成功找到并移除了状态，或者即使未找到（也认为操作已"完成"），则返回 `Ok(())`。
    /// 如果在尝试移除过程中遇到内部错误（例如，锁获取问题，尽管当前实现不太可能），则返回 `Err(String)`。
    ///
    /// # 参数
    /// * `group_id`: `&str` - 需要移除其任务状态的组的唯一ID。
    ///
    /// # 返回值
    /// * `Result<(), String>`: 操作成功时返回 `Ok(())`，否则返回包含错误描述的 `Err(String)`。
    pub async fn remove_task_state(&self, group_id: &str) -> Result<(), String> {
        info!("[任务状态管理器] 尝试为 group_id '{}' 移除任务状态...", group_id);
        // DashMap 的 remove 方法返回 Option<(K, V)>，这里是 Option<(String, Arc<RwLock<TaskDebugState>>)>
        let removed_entry = self.active_task_states.remove(group_id);

        if removed_entry.is_some() {
            info!("[任务状态管理器] 任务状态已成功为 group_id '{}' 移除。", group_id);
            Ok(())
        } else {
            warn!(
                "[任务状态管理器] 尝试为 group_id '{}' 移除任务状态，但未找到该组的状态。这可能发生在组已被清理或从未正确初始化任务状态的情况下。",
                group_id
            );
            // 即使未找到，也认为"移除操作"已尝试过且逻辑上完成（对于调用者而言，该状态确实不存在了）。
            // 如果需要区分"成功移除"和"未找到"，可以改变返回类型或错误信息。
            Ok(())
        }
    }

    /// 更新任务状态并返回更新后的状态（如果发生了实际改变）。
    ///
    /// **注意：这是P3.3.1版本的骨架实现，具体的状态更新逻辑将在P3.3.2中详细实现。**
    /// 此方法目前仅打印日志并模拟返回 `None`，表示状态未改变。
    ///
    /// # Arguments
    /// * `group_id` - 任务状态所属的组ID。
    /// * `updater_role` - 执行更新操作的客户端角色。
    /// * `payload` - 具体的业务消息 Payload (在P3.3.2的初始实现中，我们临时使用 serde_json::Value 以便灵活处理，
    ///   未来应演化为更具体的业务 Payload 枚举或 Trait 对象)。
    ///
    /// # Returns
    /// * `Some(TaskDebugState)` 如果状态被成功修改，则返回修改后状态的一个克隆。
    /// * `None` 如果状态没有发生实际改变，或者找不到对应的任务状态。
    pub async fn update_state_and_get_updated(
        &self,
        group_id: &str,
        updater_role: ClientRole,
        action_payload: BusinessActionPayload, 
    ) -> Option<TaskDebugState> {
        info!(
            "[任务状态管理器] 尝试为 group_id '{}' 更新任务状态。更新者角色: {:?}, ActionPayload: {:?}",
            group_id, updater_role, action_payload
        );

        match self.active_task_states.get_mut(group_id) { // 使用 get_mut 获取可写锁
            Some(mut task_state_entry) => {
                let mut task_state = task_state_entry.value_mut().write().await; // 获取写锁
                let mut state_changed = false;

                match action_payload {
                    BusinessActionPayload::UpdatePreCheckItem(payload) => {
                        info!("[任务状态管理器] 处理 UpdatePreCheckItem: {:?}", payload);
                        let item_id = payload.item_id.clone();
                        
                        let pre_check_item = task_state.pre_check_items
                            .entry(item_id.clone())
                            .or_insert_with(|| PreCheckItemStatus::new(item_id));

                        // 根据 updater_role 更新不同的字段
                        // 这里是一个简化的示例，实际应用中可能需要更复杂的逻辑来决定哪些字段可以被哪个角色更新
                        let current_time = Utc::now();
                        match updater_role {
                            ClientRole::OnSiteMobile => {
                                if pre_check_item.status_from_site != Some(payload.status.clone()) || pre_check_item.notes_from_site != payload.notes {
                                    pre_check_item.status_from_site = Some(payload.status);
                                    pre_check_item.notes_from_site = payload.notes;
                                    pre_check_item.last_updated = current_time;
                                    state_changed = true;
                                }
                            }
                            ClientRole::ControlCenter => {
                                if pre_check_item.status_from_control != Some(payload.status.clone()) || pre_check_item.notes_from_control != payload.notes {
                                    pre_check_item.status_from_control = Some(payload.status);
                                    pre_check_item.notes_from_control = payload.notes;
                                    pre_check_item.last_updated = current_time;
                                    state_changed = true;
                                }
                            }
                            _ => {
                                warn!("[任务状态管理器] UpdatePreCheckItem 操作被角色 {:?} 尝试，但此角色没有明确的更新权限或逻辑。", updater_role);
                            }
                        }
                    }
                    BusinessActionPayload::StartSingleTestStep(payload) => {
                        // TODO: 实现 StartSingleTestStep 的逻辑
                        info!("[任务状态管理器] TODO: 处理 StartSingleTestStep: {:?}", payload);
                        // state_changed = ...;
                    }
                    BusinessActionPayload::FeedbackSingleTestStep(payload) => {
                        // TODO: 实现 FeedbackSingleTestStep 的逻辑
                        info!("[任务状态管理器] TODO: 处理 FeedbackSingleTestStep: {:?}", payload);
                        // state_changed = ...;
                    }
                    BusinessActionPayload::ConfirmSingleTestStep(payload) => {
                        // TODO: 实现 ConfirmSingleTestStep 的逻辑
                        info!("[任务状态管理器] TODO: 处理 ConfirmSingleTestStep: {:?}", payload);
                        // state_changed = ...;
                    }
                }

                if state_changed {
                    task_state.last_updated_by_role = Some(updater_role);
                    task_state.last_update_timestamp = Utc::now();
                    task_state.version += 1; // 版本号递增
                    info!(
                        "[任务状态管理器] group_id '{}' 的任务状态已更新。新版本: {}. 最后更新者: {:?}",
                        group_id, task_state.version, updater_role
                    );
                    Some(task_state.clone()) // 返回更新后状态的克隆
                } else {
                    info!(
                        "[任务状态管理器] group_id '{}' 的任务状态未发生实际改变。",
                        group_id
                    );
                    None
                }
            }
            None => {
                warn!(
                    "[任务状态管理器] 尝试更新不存在的 group_id '{}' 的任务状态。",
                    group_id
                );
                None
            }
        }
    }

    /// 强制广播指定组的当前任务状态。
    /// 此方法主要用于特殊情况，例如由Tauri命令直接触发的状态更新后的广播。
    pub async fn force_broadcast_state(
        &self,
        group_id: &str,
        conn_manager: &Arc<super::connection_manager::ConnectionManager>, // 接收 Arc<ConnectionManager> 的引用
    ) {
        info!("[任务状态管理器] 尝试为组 '{}' 强制广播 TaskDebugState...", group_id);
        match self.get_task_state(group_id).await {
            Some(task_state_arc) => {
                let task_state_guard = task_state_arc.read().await; // 获取读锁
                // 注意：这里传递的是 task_state_guard 的引用，priv_broadcast_task_state 期望 &TaskDebugState
                self.priv_broadcast_task_state(group_id, &*task_state_guard, conn_manager).await;
            }
            None => {
                warn!(
                    "[任务状态管理器] 尝试为组 '{}' 强制广播状态失败：未找到该组的任务状态。",
                    group_id
                );
            }
        }
    }

    /// （P3.3.2 规划）私有辅助方法：将指定的 TaskDebugState 广播给指定组内的所有客户端。
    /// 此方法应由 TaskStateManager 内部在状态发生显著变化后调用。
    async fn priv_broadcast_task_state(
        &self,
        group_id: &str,
        task_state: &TaskDebugState,
        conn_manager: &super::connection_manager::ConnectionManager, 
    ) {
        info!("[任务状态管理器] 准备为组 '{}' 广播 TaskDebugState 更新...", group_id);
        let clients_in_group = conn_manager.get_group_members_for_broadcast(group_id, None).await;

        if clients_in_group.is_empty() {
            warn!("[任务状态管理器] 组 '{}' 中没有活动的客户端，取消广播 TaskDebugState。", group_id);
            return;
        }

        let serialized_state_payload = match serde_json::to_string(task_state) {
            Ok(s) => s,
            Err(e) => {
                error!(
                    "[任务状态管理器] 序列化 TaskDebugState (group_id: '{}') 以进行广播时失败: {:?}",
                    group_id, e
                );
                return;
            }
        };

        let common_ws_msg = common_models::WsMessage { 
            message_id: uuid::Uuid::new_v4().to_string(), 
            message_type: common_models::ws_payloads::TASK_STATE_UPDATE_MESSAGE_TYPE.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),    
            payload: serialized_state_payload, 
        };
        info!("[任务状态管理器] 成功序列化 TaskDebugState (group_id: '{}')，准备向 {} 个客户端广播。", group_id, clients_in_group.len());

        // 将 common_models::WsMessage 转换为 rust_websocket_utils::message::WsMessage
        let ws_msg_to_send = rust_websocket_utils::message::WsMessage {
            message_id: common_ws_msg.message_id.clone(),
            message_type: common_ws_msg.message_type.clone(),
            payload: common_ws_msg.payload.clone(),
            timestamp: common_ws_msg.timestamp,
        };

        for client_session_arc in clients_in_group {
            if let Err(e) = client_session_arc.sender.send(ws_msg_to_send.clone()).await { // 发送转换后的消息
                error!(
                    "[任务状态管理器] 向客户端 {} (组 '{}') 广播 TaskStateUpdate 失败: {:?}",
                    client_session_arc.client_id, group_id, e
                );
            }
        }
        info!("[任务状态管理器] 已完成向组 '{}' 的客户端广播 TaskDebugState。", group_id);
    }
}

// 实现 `Default` trait (默认特征) 使得 `TaskStateManager` (任务状态管理器) 可以通过调用 `TaskStateManager::default()` 来创建实例。
// 这在某些场景下非常方便，例如当 `TaskStateManager` (任务状态管理器) 作为另一个结构体的字段，
// 并且那个外部结构体派生了 `#[derive(Default)]` 时，Tauri 的状态管理宏也可能利用这一点。
impl Default for TaskStateManager {
    fn default() -> Self {
        // `Default::default()` 的实现直接调用我们已经定义的 `new()` 构造函数。
        // 这确保了通过 `default()` 创建的实例与通过 `new()` 创建的实例具有相同的初始化逻辑。
        Self::new()
    }
}

// 单元测试模块 (`#[cfg(test)]` 属性确保此模块仅在执行 `cargo test` 时被编译)
// (重要提示：在 P3.3.1 及后续的开发和测试阶段，本模块 (`tests`) 将包含更全面、更复杂的单元测试和可能的集成测试用例。
//  这些测试将用于验证：
//  - 任务状态 (`TaskDebugState` - 任务调试状态) 的正确存储和检索。
//  - 在并发访问场景下 (多个任务同时读写) 的数据安全性和一致性 (通过 `Arc<RwLock<...>>` 和 `DashMap` 保证)。
//  - 状态更新逻辑 (例如，`update_state_and_get_updated` 方法) 是否按预期工作，并能正确处理各种业务场景和边缘情况。
//  - `TaskStateManager` (任务状态管理器) 与其他核心组件 (如 `ConnectionManager` - 连接管理器, `MessageRouter` - 消息路由器) 的交互是否正确无误。
// )
#[cfg(test)]
mod tests {
    use super::*; // 从父模块 (即 `crate::ws_server::task_state_manager`) 导入所有公共成员 (如 `TaskStateManager`)，以便在测试函数中使用它们。
    use log::debug; // 在测试代码中也使用 `debug` 日志宏，方便在测试执行时输出详细的步骤信息。

    // 使用 `#[tokio::test]` 宏来标记异步测试函数。
    // 这使得我们可以在测试函数内部直接使用 `.await` 语法来调用异步代码。
    #[tokio::test]
    async fn test_skeleton_task_state_manager_creation_and_init_call() {
        // 测试目的：验证当前的 `TaskStateManager` (任务状态管理器) 的骨架实现 (P3.1.2 阶段) 能够被成功创建，
        // 并且其核心方法 (例如 `init_task_state` - 初始化任务状态)，即使它们当前只是空实现或仅包含日志记录，
        // 也能够被无错误地调用，并且不会导致程序 panic (恐慌) 或其他意外的运行时失败。
        info!("[单元测试 - TaskStateManager骨架] === 开始执行 TaskStateManager (任务状态管理器) 创建和 init_task_state (初始化任务状态) 调用测试 ===");
        
        // 步骤1: 创建 TaskStateManager (任务状态管理器) 实例
        debug!("[单元测试 - TaskStateManager骨架] 步骤1: 正在调用 TaskStateManager::new() 以创建一个新的管理器实例...");
        let manager = TaskStateManager::new(); // 调用构造函数
        debug!("[单元测试 - TaskStateManager骨架] TaskStateManager (任务状态管理器) 实例已成功创建。实例详情 (Debug输出): {:?}", manager);
        
        // 步骤2: 调用骨架实现的 `init_task_state` (初始化任务状态) 方法
        //         为测试提供一些示例性的组ID和任务ID字符串。
        let test_group_id = "单元测试专用组ID_TSM_001".to_string();
        let test_task_id = "单元测试专用任务ID_AlphaBuild".to_string();
        debug!(
            "[单元测试 - TaskStateManager骨架] 步骤2: 准备调用 manager.init_task_state() 方法。测试参数 - 组ID (group_id): '{}', 任务ID (task_id): '{}'...",
            test_group_id, test_task_id
        );
        // 调用异步方法 `init_task_state` 并使用 `.await` 等待其完成 (尽管在骨架实现中它会立即返回)。
        manager.init_task_state(test_group_id.clone(), test_task_id.clone()).await;
        info!(
            "[单元测试 - TaskStateManager骨架] manager.init_task_state() 方法已为组ID '{}' 和任务ID '{}' 成功调用并返回 (P3.1.2骨架实现，无实际操作)。",
            test_group_id, test_task_id
        );

        // 断言 (Assert) 部分：
        // 由于当前 (P3.1.2) 的 `TaskStateManager` (任务状态管理器) 是一个骨架实现，其内部并没有实际存储任何状态，
        // 因此我们无法通过检查其内部数据来断言 `init_task_state` (初始化任务状态) 是否按预期工作。
        // 本测试当前的主要目的是确保代码能够顺利编译通过，并且核心方法（即使它们是空壳或仅记录日志）
        // 在被调用时不会引发 panic (恐慌) 或其他严重的运行时错误，即验证其可执行性。
        // 在 P3.3.1 阶段的完整功能实现中，此处的断言将会更加具体和有意义。例如，我们将会检查：
        // - 调用 `init_task_state` 后，`manager.active_task_states` (活动任务状态) 集合中是否确实创建了
        //   以 `test_group_id` 为键的新条目。
        // - 该条目中的 `TaskDebugState` (任务调试状态) 是否已根据 `test_task_id` (或默认逻辑) 正确初始化。
        // - 再次调用 `init_task_state` (如果设计允许重复调用) 是否有预期的行为 (例如，不重复创建，或更新现有状态)。
        assert!(true, "这是一个针对P3.1.2骨架实现的占位断言，其设计目的是始终通过测试。它主要验证代码的基本可执行性，而非具体功能逻辑。");
        info!("[单元测试 - TaskStateManager骨架] === TaskStateManager (任务状态管理器) 创建和 init_task_state (初始化任务状态) 调用测试已成功完成。骨架功能按预期执行 (无实际状态操作，主要依赖日志进行验证)。===");
    }
} // 单元测试模块结束 