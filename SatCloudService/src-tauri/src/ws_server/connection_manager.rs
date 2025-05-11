// SatCloudService/src-tauri/src/ws_server/connection_manager.rs

//! WebSocket 连接管理。

use crate::ws_server::client_session::ClientSession;
use crate::ws_server::task_state_manager::TaskStateManager;
use common_models::enums::ClientRole;
use common_models::ws_payloads::{
    PartnerStatusPayload, RegisterPayload, RegisterResponsePayload, PARTNER_STATUS_UPDATE_MESSAGE_TYPE,
};
// 修正 WsMessage 的引入路径，移除错误的 common_models::ws_messages::WsMessage
use rust_websocket_utils::message::WsMessage;
// use rust_websocket_utils::error::WsError; // WsError 未使用，移除

use dashmap::DashMap;
use log::{debug, error, info, warn}; // 确保所有日志宏都可用
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering}; // 确保 Ordering 被导入

/// `Group` 结构体定义
/// 
/// 代表一个客户端组，通常关联到一个特定的任务。
/// 组内可以包含不同角色的客户端，例如控制中心和现场移动端。
#[derive(Debug)]
pub struct Group {
    /// 组的唯一标识。
    pub group_id: String,
    /// 与此组关联的任务ID。
    /// 在组创建时设定，用于后续的任务状态管理。
    pub task_id: String,
    /// 组内的控制中心客户端会话。
    pub control_center_client: Option<Arc<ClientSession>>,
    /// 组内的现场移动端客户端会话。
    pub on_site_mobile_client: Option<Arc<ClientSession>>,
}

impl Group {
    /// 创建一个新的 `Group` 实例。
    pub fn new(group_id: String, task_id: String) -> Self {
        Self {
            group_id,
            task_id,
            control_center_client: None,
            on_site_mobile_client: None,
        }
    }
}

/// 管理所有活动的 WebSocket 客户端会话
// 移除 Debug 派生，因为 TaskStateManager 可能未实现 Debug
#[derive(Clone)] 
pub struct ConnectionManager {
    // 将字段设为私有
    clients: Arc<DashMap<Uuid, Arc<ClientSession>>>,
    groups: Arc<DashMap<String, Arc<RwLock<Group>>>>,
    task_state_manager: Arc<TaskStateManager>,
}

impl ConnectionManager {
    /// 创建一个新的 ConnectionManager 实例
    pub fn new(task_state_manager: Arc<TaskStateManager>) -> Self {
        info!("[ConnectionManager] New instance created.");
        Self {
            clients: Arc::new(DashMap::new()),
            groups: Arc::new(DashMap::new()),
            task_state_manager,
        }
    }

    /// 添加一个新的客户端到连接管理器。
    ///
    /// # Arguments
    /// * `addr` - 客户端的 SocketAddr。
    /// * `sender` - 用于向此客户端发送消息的 mpsc Sender。
    /// * `connection_should_close` - 一个 Arc<AtomicBool>，用于从外部通知连接处理任务关闭连接。
    ///
    /// # Returns
    /// 返回新创建的 `Arc<ClientSession>`。
    pub async fn add_client(
        &self,
        addr: SocketAddr,
        sender: mpsc::Sender<WsMessage>,
        connection_should_close: Arc<AtomicBool>, // 新增参数
    ) -> Arc<ClientSession> {
        // ClientSession::new 现在需要 connection_should_close
        // 并且 ClientSession::new 内部会生成 client_id
        let client_session = Arc::new(ClientSession::new(
            addr,                       // 第一个参数是 addr
            sender,                     // 第二个参数是 sender
            connection_should_close,    // 第三个参数是 connection_should_close
        ));
        
        self.clients
            .insert(client_session.client_id, Arc::clone(&client_session)); // 使用 client_session 内部生成的 client_id
        info!(
            "[ConnectionManager] 新客户端连接成功: id={}, addr={}, 初始角色={:?}", 
            client_session.client_id, client_session.addr, *client_session.role.read().await);
        debug!("[ConnectionManager] 客户端会话详情 (添加时): {:?}", client_session);
        debug!("[ConnectionManager] 当前活动客户端总数: {}", self.clients.len());

        client_session
    }

    /// 从管理器中移除一个客户端会话。
    ///
    /// # Arguments
    /// * `client_id` - 要移除的客户端的ID。
    pub async fn remove_client(&self, client_id: &Uuid) {
        info!("[ConnectionManager] Attempting to remove client: {}", client_id);

        if let Some((_id, client_session)) = self.clients.remove(client_id) {
            info!(
                "[ConnectionManager] Client {} found and removed from active clients list.",
                client_id
            );
            // 请求关闭物理连接
            // 这一步确保如果 remove_client 是首次被调用 (例如由 HeartbeatMonitor 调用)，
            // 它会触发物理连接的关闭。
            // 如果 remove_client 是因为物理连接已关闭而被调用 (例如由 WsService 的连接处理任务结束时调用)，
            // 那么这个 store 操作是无害的 (连接已在关闭过程中或已关闭)。
            client_session
                .connection_should_close
                .store(true, Ordering::SeqCst);
            
            // Yield to allow WsService loops to potentially observe the flag immediately
            tokio::task::yield_now().await;

            debug!(
                "[ConnectionManager] Requested underlying connection to close for client: {}",
                client_id
            );

            let role_at_disconnect = client_session.role.read().await.clone();
            let group_id_option = client_session.group_id.read().await.clone();

            if let Some(group_id) = group_id_option {
                if role_at_disconnect == ClientRole::Unknown {
                    info!("[ConnectionManager] Client {} (Role: Unknown) was not in any effective group. No group cleanup needed.", client_id);
                    // 注意：这里不再直接 return，因为即使角色未知，也可能需要从 clients 列表中移除。
                    // 但由于已从 clients.remove 成功，所以主要关注点是组逻辑。
                } else {
                    info!(
                        "[ConnectionManager] Client {} (Role: {:?}) was part of group '{}'. Processing group removal and partner notification.",
                        client_id, role_at_disconnect, group_id
                    );
                    if let Some(group_lock) = self.groups.get(&group_id) {
                        let mut group = group_lock.write().await;
                        info!(
                            "[ConnectionManager] Processing group '{}' for client {} removal (Role: {:?}).",
                            group_id, client_id, role_at_disconnect
                        );

                        let mut partner_session_option: Option<Arc<ClientSession>> = None;

                        match role_at_disconnect {
                            ClientRole::ControlCenter => {
                                if let Some(cc_session) = &group.control_center_client {
                                    if cc_session.client_id == *client_id {
                                        group.control_center_client = None;
                                        info!("[ConnectionManager] Removed ControlCenter (ID: {}) from group '{}'.", client_id, group_id);
                                        partner_session_option = group.on_site_mobile_client.clone();
                                    } else {
                                        warn!("[ConnectionManager] ControlCenter in group '{}' (ID: {}) does not match client being removed (ID: {}). No change made.", 
                                            group_id, cc_session.client_id, client_id);
                                    }
                                } else {
                                    info!("[ConnectionManager] No ControlCenter was registered in group '{}' when client {} (Role: ControlCenter) disconnected.", group_id, client_id);
                                }
                            }
                            ClientRole::OnSiteMobile => {
                                if let Some(os_session) = &group.on_site_mobile_client {
                                    if os_session.client_id == *client_id {
                                        group.on_site_mobile_client = None;
                                        info!("[ConnectionManager] Removed OnSiteMobile (ID: {}) from group '{}'.", client_id, group_id);
                                        partner_session_option = group.control_center_client.clone();
                                    } else {
                                        warn!("[ConnectionManager] OnSiteMobile in group '{}' (ID: {}) does not match client being removed (ID: {}). No change made.", 
                                            group_id, os_session.client_id, client_id);
                                    }
                                } else {
                                    info!("[ConnectionManager] No OnSiteMobile was registered in group '{}' when client {} (Role: OnSiteMobile) disconnected.", group_id, client_id);
                                }
                            }
                            ClientRole::Unknown => {
                                // 已在外部处理，理论上不应进入此分支如果 role_at_disconnect 是 Unknown
                                warn!("[ConnectionManager] Client {} had Unknown role during group processing. This should not happen if filtered earlier.", client_id);
                            }
                        }

                        if let Some(partner_session) = partner_session_option {
                            info!(
                                "[ConnectionManager] Group '{}' has a partner (ID: {}). Notifying them of client {} (Role: {:?}) going offline.",
                                group_id, partner_session.client_id, client_id, role_at_disconnect
                            );
                            // client_id is &Uuid, *client_id gives Uuid which is Copy.
                            // group_id is String, needs to be cloned for payload_data if it's to be used later.
                            // However, payload_data itself is moved into the spawn, so cloning for it is fine.
                            let payload_data = PartnerStatusPayload {
                                partner_role: role_at_disconnect.clone(),
                                partner_client_id: *client_id, 
                                is_online: false,
                                group_id: group_id.clone(), // Clone group_id for the payload
                            };

                            // Prepare owned data for tokio::spawn
                            let owned_client_id_for_spawn = *client_id; // Uuid is Copy, so this creates an owned copy.
                            let group_id_for_spawn = group_id.clone(); // Clone group_id for the spawned task.
                            let partner_session_for_spawn = partner_session.clone(); // Arc is cheap to clone.

                            // Spawn a new task to send the notification to avoid blocking remove_client
                            tokio::spawn(async move {
                                // Inside this async block, use owned_client_id_for_spawn and group_id_for_spawn
                                match serde_json::to_string(&payload_data) { // payload_data is moved here
                                    Ok(json_payload) => {
                                        match WsMessage::new(
                                            PARTNER_STATUS_UPDATE_MESSAGE_TYPE.to_string(),
                                            &json_payload,
                                        ) {
                                            Ok(ws_msg) => {
                                                if let Err(e) = partner_session_for_spawn.sender.send(ws_msg).await {
                                                    error!(
                                                        "[CM SpawnedTask] Failed to send PartnerStatusUpdate (offline) to partner {} in group '{}': {}",
                                                        partner_session_for_spawn.client_id, group_id_for_spawn, e // Use cloned group_id_for_spawn
                                                    );
                                                } else {
                                                    info!(
                                                        "[CM SpawnedTask] Successfully sent PartnerStatusUpdate (offline) to partner {} for client {}.",
                                                        partner_session_for_spawn.client_id, owned_client_id_for_spawn // Use owned_client_id_for_spawn
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                error!("[CM SpawnedTask] Failed to create WsMessage for PartnerStatusUpdate (offline) for group '{}': {}", group_id_for_spawn, e); // Use cloned group_id_for_spawn
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("[CM SpawnedTask] Failed to serialize PartnerStatusPayload (offline) for group '{}': {}", group_id_for_spawn, e); // Use cloned group_id_for_spawn
                                    }
                                }
                            });
                            // After tokio::spawn, the original group_id is still valid in this scope (if it wasn't moved to payload_data directly, but it was cloned for it)
                            // and client_id (&Uuid) is also still valid.
                        } else {
                            info!("[ConnectionManager] Client {} (Role: {:?}) had no partner in group '{}' at disconnect. No offline notification sent.", client_id, role_at_disconnect, group_id);
                        }

                        // 检查组是否为空，如果为空则移除组并处理任务状态
                        if group.control_center_client.is_none() && group.on_site_mobile_client.is_none() {
                            info!(
                                "[ConnectionManager] Group '{}' is now empty after client {} disconnected. Removing group.",
                                group_id, client_id
                            );
                            // 从 self.groups 移除组之前先 drop group 的写锁
                            let task_id_for_removal = group.task_id.clone(); // 克隆 task_id
                            drop(group); //显式 drop 写锁

                            if self.groups.remove(&group_id).is_some() {
                                info!("[ConnectionManager] Successfully removed empty group '{}' from ConnectionManager.", group_id);
                                // TODO: 在P3.3.1中，这里应该调用 self.task_state_manager.remove_task_state(&group_id, &task_id_for_removal).await;
                                // 暂时只记录日志
                                debug!(
                                    "[ConnectionManager] Placeholder for TaskStateManager: Would remove task state for group_id: '{}', task_id: '{}'",
                                    group_id, task_id_for_removal
                                );
                            } else {
                                warn!("[ConnectionManager] Failed to remove group '{}' from groups map. It might have been removed by another concurrent operation.", group_id);
                            }
                        } else {
                            info!("[ConnectionManager] Group '{}' still has members after client {} disconnected.", group_id, client_id);
                        }
                    } else {
                        warn!("[ConnectionManager] Group '{}' not found for client {} during removal. Partner notification/group cleanup skipped.", group_id, client_id);
                    }
                }
            } else {
                info!("[ConnectionManager] Client {} was not associated with any group at disconnect. No group-specific cleanup needed.", client_id);
            }
        } else {
            warn!(
                "[ConnectionManager] Attempted to remove client {}, but it was not found in active clients list. It might have been already removed by another process (e.g., connection drop handler). No further action taken by this call.",
                client_id
            );
            // 如果客户端已不在 active_clients 中，则不执行任何操作，
            // 因为相关的清理（包括伙伴通知和组移除）应该已经由第一次成功的 remove_client 调用处理了。
        }
        debug!(
            "[ConnectionManager] Remove client {} operation completed. Current active client count: {}",
            client_id,
            self.clients.len()
        );
        // 因为方法返回类型是 ()，所以不需要显式返回 None 或 Some
    }
    
    // 根据用户提供的版本，join_group 返回 Result<RegisterResponsePayload, RegisterResponsePayload>
    pub async fn join_group(
        &self,
        client_session: Arc<ClientSession>,
        payload: RegisterPayload,
    ) -> Result<RegisterResponsePayload, RegisterResponsePayload> {
        let client_id = client_session.client_id;
        let group_id = payload.group_id.clone();
        let requested_role = payload.role.clone();
        // 使用 task_id_payload 变量名以示清晰
        let task_id_payload = payload.task_id.clone();

        info!(
            "[ConnectionManager] 客户端 {} 请求加入组 '{}' (任务 '{}')，角色为: {:?}",
            client_id, group_id, task_id_payload, requested_role
        );

        if requested_role == ClientRole::Unknown {
            warn!(
                "[ConnectionManager] 客户端 {} 尝试以 Unknown 角色加入组 '{}'。拒绝请求。",
                client_id, group_id
            );
            return Err(RegisterResponsePayload {
                success: false,
                message: Some("不允许以 'Unknown' 角色加入组。".to_string()),
                assigned_client_id: client_id,
                effective_group_id: None,
                effective_role: None,
            });
        }

        let group_arc_rwlock = self
            .groups
            .entry(group_id.clone())
            .or_insert_with(|| {
                info!(
                    "[ConnectionManager] 组 '{}' (任务 '{}') 不存在，将创建新组。",
                    group_id, task_id_payload // 使用 task_id_payload
                );
                Arc::new(RwLock::new(Group::new(group_id.clone(), task_id_payload.clone())))
            })
            .value()
            .clone();

        let mut group = group_arc_rwlock.write().await;
        info!(
            "[ConnectionManager] 获取到组 '{}' 的写锁，处理客户端 {} 的加入请求。",
            group_id, client_id
        );

        // 修复 E0382 borrow error
        if group.task_id != task_id_payload { // 使用 task_id_payload
            warn!(
                "[ConnectionManager] 客户端 {} 尝试加入组 '{}'，但提供的任务ID '{}'与组内记录的任务ID '{}'不匹配。拒绝请求。",
                client_id, group_id, task_id_payload, group.task_id // group.task_id 在 drop 前使用，安全
            );
            // 在 drop(group) 之前克隆 group.task_id
            let actual_group_task_id_for_error_msg = group.task_id.clone();
            
            drop(group); // 释放写锁
            
            if let Some(entry) = self.groups.get(&group_id) {
                let temp_group_guard = entry.value().read().await;
                if temp_group_guard.control_center_client.is_none() && temp_group_guard.on_site_mobile_client.is_none() {
                    info!("[ConnectionManager] 由于任务ID不匹配且组 '{}' 为空，将移除此临时创建的组。", group_id);
                    drop(temp_group_guard); // 释放读锁
                    self.groups.remove(&group_id);
                    // 注释掉对 remove_task_state 的调用
                    /* 
                    if let Err(e) = self.task_state_manager.remove_task_state(&group_id).await {
                         error!("[ConnectionManager] 清理临时组 '{}' 的任务状态失败: {:?}", group_id, e);
                    }
                    */
                    info!("[ConnectionManager] 对组 '{}' (因task_id不匹配创建的临时组) 的 remove_task_state 调用已暂时注释。", group_id);
                }
            }

            return Err(RegisterResponsePayload {
                success: false,
                message: Some(format!(
                    "任务ID '{}' 与组内记录的任务ID '{}' 不匹配。",
                    task_id_payload, actual_group_task_id_for_error_msg // 使用克隆后的值
                )),
                assigned_client_id: client_id,
                effective_group_id: None,
                effective_role: None,
            });
        }

        match requested_role {
            ClientRole::ControlCenter => {
                if let Some(existing_cc) = &group.control_center_client {
                    if existing_cc.client_id != client_id {
                        warn!(
                            "[ConnectionManager] 角色冲突：客户端 {} 尝试以 ControlCenter 加入组 '{}'，但该组已存在 ControlCenter (ID: {}).",
                            client_id, group_id, existing_cc.client_id
                        );
                        return Err(RegisterResponsePayload {
                            success: false,
                            message: Some(format!("组 '{}' 已存在控制中心客户端。", group_id)),
                            assigned_client_id: client_id,
                            effective_group_id: None,
                            effective_role: None,
                        });
                    } else {
                        info!("[ConnectionManager] 客户端 {} (ControlCenter) 已在组 '{}' 中，重新确认角色。", client_id, group_id);
                    }
                }
            }
            ClientRole::OnSiteMobile => {
                if let Some(existing_osm) = &group.on_site_mobile_client {
                    if existing_osm.client_id != client_id {
                        warn!(
                            "[ConnectionManager] 角色冲突：客户端 {} 尝试以 OnSiteMobile 加入组 '{}'，但该组已存在 OnSiteMobile (ID: {}).",
                            client_id, group_id, existing_osm.client_id
                        );
                        return Err(RegisterResponsePayload {
                            success: false,
                            message: Some(format!("组 '{}' 已存在现场移动端客户端。", group_id)),
                            assigned_client_id: client_id,
                            effective_group_id: None,
                            effective_role: None,
                        });
                    } else {
                         info!("[ConnectionManager] 客户端 {} (OnSiteMobile) 已在组 '{}' 中，重新确认角色。", client_id, group_id);
                    }
                }
            }
            ClientRole::Unknown => { /* 已在前面处理 */ }
        }

        let is_newly_effective_group = group.control_center_client.is_none() && group.on_site_mobile_client.is_none();
        if is_newly_effective_group { 
             info!(
                "[ConnectionManager] 组 '{}' (任务 '{}') 是新创建或首次有效化，将调用 TaskStateManager::init_task_state。",
                group.group_id, group.task_id
            );
            // init_task_state 不返回 Result，直接调用
            self.task_state_manager
                .init_task_state(group.group_id.clone(), group.task_id.clone())
                .await;
            info!(
                 "[ConnectionManager] 已尝试为组 '{}' 调用 TaskStateManager::init_task_state。",
                 group.group_id
            );
        }

        {
            let mut role_guard = client_session.role.write().await;
            *role_guard = requested_role.clone();
            let mut group_id_guard = client_session.group_id.write().await;
            *group_id_guard = Some(group_id.clone());
            info!(
                "[ConnectionManager] 客户端 {} 的会话状态已更新：角色 -> {:?}，组ID -> {}",
                client_id, requested_role, group_id
            );
        }

        let mut existing_partner_session: Option<Arc<ClientSession>> = None;
        match requested_role {
            ClientRole::ControlCenter => {
                existing_partner_session = group.on_site_mobile_client.clone();
                group.control_center_client = Some(client_session.clone());
            }
            ClientRole::OnSiteMobile => {
                existing_partner_session = group.control_center_client.clone();
                group.on_site_mobile_client = Some(client_session.clone());
            }
            ClientRole::Unknown => { /* 不应发生 */ }
        }
        info!(
            "[ConnectionManager] 组 '{}' 的成员信息已更新，客户端 {} ({:?}) 已加入。",
            group_id, client_id, requested_role
        );

        if let Some(partner_session) = &existing_partner_session {
            info!(
                "[ConnectionManager] 检测到组 '{}' 中存在伙伴 (ID: {}, Role: {:?})，准备通知其新成员 {} (Role: {:?}) 已上线。",
                group_id, partner_session.client_id, partner_session.role.read().await.clone(), client_id, requested_role
            );
            let payload_to_partner = PartnerStatusPayload {
                partner_role: requested_role.clone(),
                partner_client_id: client_id,
                is_online: true,
                group_id: group_id.clone(),
            };
            match serde_json::to_string(&payload_to_partner) {
                Ok(json_payload) => {
                    let ws_msg_result = WsMessage::new(
                        PARTNER_STATUS_UPDATE_MESSAGE_TYPE.to_string(),
                        &json_payload,
                    );
                    match ws_msg_result {
                        Ok(ws_msg) => {
                            if let Err(e) = partner_session.sender.send(ws_msg).await {
                                error!(
                                    "[ConnectionManager] 发送新成员上线通知给伙伴客户端 {} 失败: {:?}. 伙伴可能已断开。",
                                    partner_session.client_id, e
                                );
                            } else {
                                info!(
                                    "[ConnectionManager] 已成功发送新成员上线通知 (客户端 {} 上线) 给伙伴客户端 {}.",
                                    client_id, partner_session.client_id
                                );
                            }
                        }
                        Err(e) => {
                            error!(
                                "[ConnectionManager] 创建 PartnerStatusPayload (新成员上线通知) WsMessage 失败: {:?}",
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    error!("[ConnectionManager] Failed to serialize PartnerStatusPayload (new member online notification) for group '{}': {}", group_id, e);
                }
            }
        } else {
            info!("[ConnectionManager] 客户端 {} ({:?}) 加入组 '{}' 时，组内尚无伙伴。", client_id, requested_role, group_id);
        }

        if let Some(partner_already_in_group) = &existing_partner_session {
            let partner_role_in_group = partner_already_in_group.role.read().await.clone();
             info!(
                "[ConnectionManager] 准备通知新加入的客户端 {} (Role: {:?}) 关于其伙伴 (ID: {}, Role: {:?}) 的在线状态。",
                client_id, requested_role, partner_already_in_group.client_id, partner_role_in_group
            );
            let payload_to_current_client = PartnerStatusPayload {
                partner_role: partner_role_in_group,
                partner_client_id: partner_already_in_group.client_id,
                is_online: true,
                group_id: group_id.clone(),
            };
            match serde_json::to_string(&payload_to_current_client) {
                Ok(json_payload) => {
                    let ws_msg_result = WsMessage::new(
                        PARTNER_STATUS_UPDATE_MESSAGE_TYPE.to_string(),
                        &json_payload,
                    );
                    match ws_msg_result {
                        Ok(ws_msg) => {
                            if let Err(e) = client_session.sender.send(ws_msg).await {
                                error!(
                                    "[ConnectionManager] 发送伙伴在线状态通知给当前客户端 {} 失败: {:?}.",
                                    client_id, e
                                );
                            } else {
                                 info!(
                                    "[ConnectionManager] 已成功发送伙伴在线状态通知给当前客户端 {}.",
                                    client_id
                                );
                            }
                        }
                        Err(e) => {
                            error!(
                                "[ConnectionManager] 创建 PartnerStatusPayload (partner online status notification) WsMessage 失败: {:?}",
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    error!("[ConnectionManager] Failed to serialize PartnerStatusPayload (partner online status notification) for group '{}': {}", group_id, e);
                }
            }
        }

        let success_msg = format!("已成功加入组 '{}'。", group_id);
        info!(
            "[ConnectionManager] 客户端 {} 成功加入组 '{}'。响应: success=true, message='{}'",
            client_id, group_id, success_msg
        );

        Ok(RegisterResponsePayload {
            success: true,
            message: Some(success_msg),
            assigned_client_id: client_id,
            effective_group_id: Some(group_id),
            effective_role: Some(requested_role),
        })
    }

    /// 获取当前所有活动客户端会话的快照。
    /// 此方法用于内部监控，如心跳检查。
    pub fn get_all_client_sessions(&self) -> Vec<Arc<ClientSession>> {
        self.clients.iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    pub fn get_client_count(&self) -> usize {
        self.clients.len()
    }
    
    // 根据用户提供的版本，此方法存在，但可能未使用或用于测试
    #[allow(dead_code)]
    pub async fn get_group(&self, group_id: &str) -> Option<tokio::sync::OwnedRwLockReadGuard<Group>> {
        if let Some(group_entry) = self.groups.get(group_id) {
            let group_arc_rwlock = group_entry.value().clone();
            Some(group_arc_rwlock.read_owned().await)
        } else {
            None
        }
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        info!("[ConnectionManager] Creating default instance.");
        Self::new(Arc::new(TaskStateManager::default()))
    }
} 