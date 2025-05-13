// SatControlCenter/src/app/core/models/tauri-events.model.ts

// 首先定义基础的 ClientRole 枚举，与 common_models::enums::ClientRole 对应
export enum ClientRole {
    ControlCenter = "ControlCenter",
    OnSiteMobile = "OnSiteMobile",
    Unknown = "Unknown",
}

/**
 * @interface TaskDebugState
 * @description
 * 对应 common_models::task_state::TaskDebugState。
 * 表示一个正在进行的调试任务的完整共享状态。
 * 注意：此处的字段应与 Rust 结构体保持同步。
 * 根据项目需求，这里可能包含许多嵌套的复杂类型。
 * 作为初始版本，我们先定义基本框架。
 */
export interface TaskDebugState {
    /** 任务的唯一标识符。 */
    task_id: string;
    /** 任务的当前阶段或总体状态的描述性文字。 */
    current_phase?: string; // 示例字段
    /** 预检查项的状态集合。键为预检查项的ID。 */
    pre_check_items?: { [key: string]: PreCheckItemState }; // 示例字段
    /** 单体测试步骤的状态集合。键为 'device_id_step_id' 格式。 */
    single_test_steps?: { [key: string]: SingleTestStepState }; // 示例字段
    /** 联锁条件的状态集合。键为联锁条件的ID。 */
    interlock_conditions?: { [key: string]: InterlockConditionState }; // 示例字段
    /** 记录最后是哪个角色的客户端更新了状态。 */
    last_updated_by_role?: ClientRole;
    /** 记录最后更新的时间戳 (Unix epoch milliseconds or ISO string)。 */
    last_update_timestamp: number | string;
    /** 其他特定于任务的数据... */
    // 根据 common_models::task_state::TaskDebugState 的实际定义来填充更多字段
    // 例如：
    // devices_status?: { [deviceId: string]: DeviceOverallStatus };
    // global_logs?: Array<{ timestamp: number, message: string, source: string }>;
}

// --- 辅助接口 (TaskDebugState 的组成部分示例) ---
// 这些也应该与 common_models 中对应的 Rust 结构体同步

/**
 * @interface PreCheckItemState
 * @description 单个预检查项的状态。
 */
export interface PreCheckItemState {
    item_id: string;
    description?: string;
    status_from_site?: string; // 例如: "Pending", "Completed_Success", "Completed_Fail"
    notes_from_site?: string;
    confirmed_by_control?: boolean;
    confirmation_timestamp?: number | string;
}

/**
 * @interface SingleTestStepState
 * @description 单个单体测试步骤的状态。
 */
export interface SingleTestStepState {
    step_id: string; // 通常是 device_id_step_name
    device_id: string;
    description?: string;
    command_from_control?: string; // 例如: "RUN", "STOP", "RESET"
    status_from_site?: string; // 例如: "NotStarted", "InProgress", "Completed_Success", "Failed"
    result_data_from_site?: any; // 具体的测试结果数据
    feedback_message_from_site?: string;
    confirmed_by_control?: boolean;
    confirmation_timestamp?: number | string;
}

/**
 * @interface InterlockConditionState
 * @description 单个联锁条件的状态。
 */
export interface InterlockConditionState {
    condition_id: string;
    description?: string;
    is_met?: boolean;
    monitored_value?: any;
    last_checked_timestamp?: number | string;
}

// --- Tauri 事件负载接口 ---

/**
 * @interface WsConnectionStatusEvent
 * @description
 * 对应 SatControlCenter/src-tauri/src/event.rs 中的 WsConnectionStatusEvent。
 * WebSocket 连接状态事件的负载。
 */
export interface WsConnectionStatusEvent {
    /** 指示是否已连接。 */
    connected: boolean;
    /** 如果连接成功，云端分配的客户端 ID。 */
    client_id?: string;
    /** 如果连接失败，相关的错误信息。 */
    error_message?: string;
}

/**
 * @interface WsRegistrationStatusEvent
 * @description
 * 对应 SatControlCenter/src-tauri/src/event.rs 中的 WsRegistrationStatusEvent。
 * WebSocket 客户端注册状态事件的负载。
 */
export interface WsRegistrationStatusEvent {
    /** 指示注册是否成功。 */
    success: boolean;
    /** 可选的附加信息，例如成功消息或失败原因。 */
    message?: string;
    /** 客户端尝试注册或已成功注册到的组 ID。 */
    group_id: string;
    /** 如果注册成功，服务器分配给此客户端的唯一ID (UUID string)。 */
    assigned_client_id?: string;
}

/**
 * @interface WsPartnerStatusEvent
 * @description
 * 对应 SatControlCenter/src-tauri/src/event.rs 中的 WsPartnerStatusEvent。
 * WebSocket 伙伴（例如现场端）状态事件的负载。
 */
export interface WsPartnerStatusEvent {
    /** 发生状态变化的伙伴的角色。 */
    partner_role: ClientRole;
    /** 发生状态变化的伙伴的客户端 ID (UUID string)。 */
    partner_client_id: string;
    /** 指示伙伴是否在线 (true 表示上线/加入组，false 表示下线/离开组)。 */
    is_online: boolean;
    /** 相关的组 ID。 */
    group_id: string;
}

/**
 * @interface LocalTaskStateUpdatedEvent
 * @description
 * 对应 SatControlCenter/src-tauri/src/event.rs 中的 LocalTaskStateUpdatedEvent。
 * 本地任务状态因从云端同步而更新的事件负载。
 */
export interface LocalTaskStateUpdatedEvent {
    /** 从云端接收到的、最新的完整任务调试状态。 */
    new_state: TaskDebugState;
}

/**
 * @interface EchoResponseEventPayload
 * @description
 * 对应 SatControlCenter/src-tauri/src/event.rs 中的 EchoResponseEventPayload。
 * Echo 测试响应事件的负载。
 */
export interface EchoResponseEventPayload {
    /** Echo 的内容。 */
    content: string;
} 