// SatCloudService/src-tauri/src/ws_server/heartbeat_monitor.rs

//! 心跳监视器模块。
//!
//! 该模块的核心职责是定期检查所有已连接 WebSocket 客户端的活跃状态。
//! 如果某个客户端在预设定的不活动时间（超时阈值）内没有任何通信（包括 Ping 消息），
//! 心跳监视器会认为该客户端已失联或不再响应，并会启动断开该客户端连接的流程。
//! 这样做有助于及时释放服务器资源，防止因大量僵尸连接耗尽系统能力，从而维护整体服务的稳定性和健康。

use crate::ws_server::connection_manager::ConnectionManager; // 引入连接管理器，用于获取客户端列表和移除客户端
// use crate::ws_server::client_session::ClientSession; // 移除未使用的导入
// use crate::config::WebSocketConfig; // 配置信息现在通过构造函数传入，而不是直接从模块读取
use chrono::Utc; // 引入 chrono 库的 Utc 时间，用于获取当前时间戳，进行时间比较
use log::{info, warn, debug}; // 引入日志宏 (info, warn, debug)，用于在程序不同阶段输出诊断和状态信息
use std::sync::Arc; // 引入原子引用计数 Arc，用于在不同异步任务间安全地共享对 ConnectionManager 等状态的所有权
use std::time::Duration; // 引入标准库的时间间隔类型 Duration，用于表示超时和检查周期等
use tokio::time::sleep; // 引入 Tokio 的异步睡眠功能，用于在主循环中实现定时执行检查任务

/// `HeartbeatMonitor` 结构体定义。
/// 
/// 它封装了心跳检测机制所需的所有状态和依赖项，包括：
/// - 对 `ConnectionManager` 的共享引用，以便访问客户端列表并请求移除超时的客户端。
/// - 客户端允许的最大不活动时间 (`client_timeout_duration`)。
/// - 执行超时检查任务的周期性时间间隔 (`check_interval`)。
// 注意：如果 `ConnectionManager` 未实现 `Debug` trait，则直接在此派生 `#[derive(Debug)]` 可能会导致编译错误。
// 如果需要调试打印 `HeartbeatMonitor` 实例，应确保其所有成员都支持 `Debug`。
pub struct HeartbeatMonitor {
    /// 对 `ConnectionManager` 的共享引用 (`Arc<ConnectionManager>`)。
    /// `HeartbeatMonitor` 通过此引用来获取当前所有活动客户端的会话列表，
    /// 并在检测到客户端超时后，调用 `ConnectionManager` 的方法来移除相应的客户端会话。
    connection_manager: Arc<ConnectionManager>,
    
    /// 客户端被判断为超时的最大不活动持续时间 (`std::time::Duration`)。
    /// 如果一个客户端在超过此设定时长的时间内没有向服务器发送任何消息
    /// (其 `ClientSession` 中的 `last_seen` 时间戳未得到更新)，
    /// 则该客户端将被视为超时。
    client_timeout_duration: Duration,
    
    /// 心跳监视器执行其周期性超时检查任务的时间间隔 (`std::time::Duration`)。
    /// `HeartbeatMonitor` 的主运行循环会每隔这么长时间唤醒一次，
    /// 然后调用 `check_for_timed_out_clients` 方法来遍历所有客户端并检查其是否超时。
    /// 此间隔通常应远小于 `client_timeout_duration`，以确保能及时发现超时。
    check_interval: Duration,
}

impl HeartbeatMonitor {
    /// 创建一个新的 `HeartbeatMonitor` 实例。
    ///
    /// # 参数
    /// * `connection_manager`: `Arc<ConnectionManager>` - 对 `ConnectionManager` 实例的共享原子引用计数指针。
    ///   这是心跳监视器执行其职责所必需的核心依赖。
    /// * `client_timeout_duration`: `Duration` - 客户端在被判断为超时之前允许的最大不活动时间。
    /// * `check_interval`: `Duration` - 心跳监视器执行其超时检查逻辑的频率。
    ///
    /// # 返回
    /// 返回一个根据传入参数初始化完成的 `HeartbeatMonitor` 实例。
    pub fn new(
        connection_manager: Arc<ConnectionManager>,
        client_timeout_duration: Duration,
        check_interval: Duration,
    ) -> Self {
        info!(
            "[心跳监视器] 正在创建并初始化新的 HeartbeatMonitor 实例。客户端超时阈值: {:?}，检查周期: {:?}",
            client_timeout_duration,
            check_interval
        );
        Self {
            connection_manager, // 存储对连接管理器的共享引用
            client_timeout_duration, // 设置客户端超时时长
            check_interval,      // 设置检查间隔
        }
    }

    /// 启动心跳监视器的主运行循环。
    /// 
    /// 此方法是一个异步函数 (`async fn`)，设计为在后台持续运行。
    /// 一旦被调用 (通常通过 `tokio::spawn` 在应用启动时派生一个新的异步任务来执行它)，
    /// 它将进入一个无限循环。在每次循环迭代中，它会首先异步地暂停执行（`sleep`）一段
    /// 由 `self.check_interval` 定义的时间，然后调用 `check_for_timed_out_clients` 方法来
    /// 执行实际的客户端超时检查和处理逻辑。
    /// 此循环会一直持续，直到其所在的异步任务被显式取消或程序终止。
    pub async fn run(self) {
        info!("[心跳监视器] 心跳监视器后台运行循环已成功启动。将按设定周期 ({:?}) 检查客户端连接状态。", self.check_interval);
        loop {
            // 异步等待（暂停执行）由 `self.check_interval` 指定的时间长度。
            // 这确保了超时检查不会过于频繁地执行，从而避免不必要的CPU消耗。
            sleep(self.check_interval).await; 
            
            debug!("[心跳监视器] 定时检查周期已到，正在准备执行客户端超时检查...");
            // 调用内部方法来执行实际的超时检查和处理逻辑。
            self.check_for_timed_out_clients().await; 
            debug!("[心跳监视器] 本轮客户端超时检查已完成。等待下一个检查周期。");
        }
    }

    /// 异步地检查所有当前活动的客户端，识别并处理那些可能已经超时的连接。
    ///
    /// 此方法是心跳监视器的核心工作单元，其主要步骤包括：
    /// 1. 调用 `ConnectionManager::get_all_client_sessions()` 来获取当前所有活动客户端会话的一个快照列表。
    ///    这确保了检查过程基于一个一致的客户端集合，即使在检查期间有新的连接或断开发生。
    /// 2. 如果没有活动的客户端，则直接返回，避免不必要的工作。
    /// 3. 获取当前的UTC时间戳，作为比较的基准。
    /// 4. 遍历快照中的每一个 `ClientSession`：
    ///    a. 异步读取该会话中记录的 `last_seen` 时间戳（表示服务器最后一次收到该客户端消息的时间）。
    ///    b. 将配置的 `client_timeout_duration` (类型为 `std::time::Duration`) 转换为 `chrono::Duration`，
    ///       以便能与 `chrono::DateTime<Utc>` 类型的时间戳进行比较。如果转换失败（理论上不应发生，除非配置值异常），
    ///       则会记录警告并使用一个安全的默认超时值。
    ///    c. 计算当前时间与 `last_seen` 时间戳之间的时间差。
    ///    d. 如果这个时间差大于转换后的 `chrono_timeout`，则判定该客户端已超时。
    /// 5. 对于每个被判定为超时的客户端：
    ///    a. 记录一条警告级别的日志，包含客户端的地址、ID、最后活跃时间以及配置的超时时长，以便追踪。
    ///    b. 异步调用 `self.connection_manager.remove_client(&client_id)` 方法来处理该客户端的移除。
    ///       `remove_client` 方法会负责将会话从活动列表中移除，通知同组伙伴（如果存在）该客户端已下线，
    ///       并最终请求关闭与该客户端关联的底层 WebSocket 连接。
    ///    c. 记录一条信息日志，表明对该超时客户端的移除请求已发出。
    /// 6. 如果客户端未超时，则仅在调试日志级别记录其仍然活跃的信息。
    async fn check_for_timed_out_clients(&self) {
        // 从 ConnectionManager 获取当前所有活动客户端会话的快照（一个 Vec<Arc<ClientSession>>）。
        // 这允许我们在一个固定的客户端列表上进行操作，即使在检查过程中 ConnectionManager 的状态发生变化。
        let clients_snapshot = self.connection_manager.get_all_client_sessions();

        if clients_snapshot.is_empty() {
            debug!("[心跳监视器] 当前系统中没有活动的客户端连接，无需执行超时检查。");
            return; // 如果没有活动客户端，则提前返回，避免不必要的后续操作。
        }

        debug!(
            "[心跳监视器] 开始对 {} 个当前活动的客户端连接进行超时状态检查。配置的超时阈值: {:?}",
            clients_snapshot.len(),
            self.client_timeout_duration
        );

        let now = Utc::now(); // 获取当前的协调世界时 (UTC) 时间戳，作为所有比较的基准。

        for client_session in &clients_snapshot { // 遍历从快照中获取的每个客户端会话的共享引用 - 使用 & 避免移动
            let client_id = client_session.client_id; // 获取当前正在检查的客户端的唯一ID
            
            // 异步读取此客户端会话中记录的最后活跃时间 (`last_seen`)。
            // `last_seen` 是一个被 `RwLock` 保护的 `DateTime<Utc>`，因此读取需要 `.await`。
            let last_seen_dt = *client_session.last_seen.read().await; 
            
            // 将配置中 `std::time::Duration` 类型的 `client_timeout_duration` 转换为 `chrono::Duration` 类型，
            // 以便能与 `chrono::DateTime<Utc>` 类型的时间戳 (如 `now` 和 `last_seen_dt`) 进行安全的算术运算和比较。
            // `chrono::Duration::from_std` 可能会因输入 `Duration` 为负或过大而返回错误。
            // 假设我们的 `client_timeout_duration` 配置总是正数且在合理范围内。
            let chrono_timeout_threshold = match chrono::Duration::from_std(self.client_timeout_duration) {
                Ok(duration) => duration, // 转换成功，使用配置的超时时长
                Err(e) => { // 如果转换失败 (例如，配置值异常导致无法转换为有效的 chrono::Duration)
                    warn!(
                        "[心跳监视器] 无法将配置的 client_timeout_duration ({:?}) 从 std::time::Duration 转换为 chrono::Duration: {}. \
                        本次检查将针对客户端 {} (ID: {}) 使用一个备用的60秒超时阈值。请检查配置。",
                        self.client_timeout_duration, e, client_session.addr, client_id
                    );
                    chrono::Duration::seconds(60) // 提供一个安全的、硬编码的备用超时值 (例如60秒)
                }
            };

            // 核心超时判断逻辑：
            // 计算当前时间 `now` 与客户端最后活跃时间 `last_seen_dt` 之间的时间差 (`signed_duration_since`)。
            // 如果这个差值大于我们确定的超时阈值 `chrono_timeout_threshold`，则认为客户端已超时。
            if now.signed_duration_since(last_seen_dt) > chrono_timeout_threshold {
                // 客户端被判定为超时
                warn!(
                    "[心跳监视器] 检测到客户端 {} (ID: {}) 已超时！最后活跃时间: {:?} (UTC), 当前时间: {:?} (UTC), 配置超时阈值: {:?}. 将启动移除流程...",
                    client_session.addr, // 客户端的网络地址，有助于定位问题
                    client_id,           // 客户端的唯一标识符
                    last_seen_dt,        // 记录的客户端最后一次发送消息的时间
                    now,                 // 当前检查时间
                    self.client_timeout_duration // 用户在配置中设定的原始超时时长 (std::time::Duration)
                );
                
                // 异步调用 ConnectionManager 的 `remove_client` 方法来处理此超时客户端的移除。
                // `remove_client` 方法负责将会话从活动列表中删除，通知组内伙伴（如果存在），
                // 并最终请求关闭与该客户端关联的底层 WebSocket 连接。
                // 这是一个异步操作，所以我们使用 `.await` 来等待它完成（或在其内部派生任务）。
                self.connection_manager.remove_client(&client_id).await;
                
                // 在 `remove_client` 调用完成后记录日志，表明对该超时客户端的处理请求已发出。
                // 注意：`remove_client` 本身可能只是启动了清理过程，物理连接的关闭可能稍有延迟。
                info!(
                    "[心跳监视器] 已为超时客户端 {} (ID: {}) 调用 ConnectionManager 的移除处理方法。",
                    client_session.addr, client_id
                );
            } else {
                // 如果客户端未超时，仅在调试日志级别记录其仍然保持活跃的信息，以避免在正常情况下产生过多日志。
                debug!(
                    "[心跳监视器] 客户端 {} (ID: {}) 状态正常，未超时。最后活跃于: {:?} (UTC), 当前时间: {:?} (UTC).",
                    client_session.addr, client_id, last_seen_dt, now
                );
            }
        }
        debug!("[心跳监视器] 对所有 {} 个活动客户端的超时状态检查已全部完成。", clients_snapshot.len());
    }
} 