// TypeScript 模型定义，用于描述从 Tauri Rust 后端通过事件传递到 Angular 前端的各种负载 (payload) 结构。
// 这些接口确保了前端在处理事件数据时的类型安全。

import { TaskDebugState } from './task-debug-state.model';

/**
 * @description WebSocket 连接状态事件的负载接口。
 *              当 `WS_CONNECTION_STATUS_EVENT` 事件被触发时，其 payload 应符合此结构。
 *              对应 Rust 结构体: `WsConnectionStatusEventPayload` (在 Rust 端的 `event.rs` 中定义)
 */
export interface WsConnectionStatusEventPayload {
  /**
   * @description 表示 WebSocket 是否已成功连接到云服务。
   *              `true` 表示已连接, `false` 表示未连接或连接已断开。
   */
  connected: boolean;
  /**
   * @description （可选）如果连接成功，云端分配给此客户端的唯一标识符。
   *              如果连接失败或断开，此字段可能为 `null` 或未定义。
   */
  client_id?: string | null;
  /**
   * @description （可选）如果连接失败或在连接过程中发生错误，此字段可能包含错误描述信息。
   */
  error_message?: string | null;
}

/**
 * @description 客户端注册状态事件的负载接口。
 *              当 `WS_REGISTRATION_STATUS_EVENT` 事件被触发时，其 payload 应符合此结构。
 *              对应 Rust 结构体: `WsRegistrationStatusEventPayload` (在 Rust 端的 `event.rs` 中定义)
 */
export interface WsRegistrationStatusEventPayload {
  /**
   * @description 表示客户端向云服务注册是否成功。
   */
  success: boolean;
  /**
   * @description （可选）提供关于注册结果的附加文本消息（例如，成功信息或失败原因）。
   */
  message?: string | null;
  /**
   * @description （可选）如果注册成功，这是客户端被分配到的或确认加入的任务组ID。
   */
  group_id?: string | null;
  /**
   * @description （可选）如果注册成功，这是客户端关联的任务ID。
   */
  task_id?: string | null;
  /**
   * @description （可选）云端分配给客户端的唯一标识，注册成功时返回。
   *              此字段在 Rust 端的 `WsRegistrationStatusEventPayload` 中名为 `assigned_client_id`。
   */
  assigned_client_id?: string | null; 
}


/**
 * @description 伙伴客户端状态更新事件的负载接口。
 *              当 `WS_PARTNER_STATUS_EVENT` 事件被触发时，其 payload 应符合此结构。
 *              注意：此定义与 `TauriListenerService` 中的 `PartnerInfo` 接口应保持一致或关联。
 *              对应 Rust 结构体: `WsPartnerStatusEventPayload` (在 Rust 端的 `event.rs` 中定义)
 */
export interface WsPartnerStatusEventPayload { // 此接口之前在 tauri-listener.service.ts 中有相似定义 PartnerInfo
  /**
   * @description 伙伴客户端的角色 (例如，"ControlCenter" 或 "OnSiteMobile")。
   */
  partner_role: string;
  /**
   * @description 伙伴客户端当前的在线状态。
   */
  is_online: boolean;
  /**
   * @description （可选）伙伴客户端的唯一ID。
   */
  partner_client_id?: string; 
  /**
   * @description （可选）伙伴客户端所在的任务组ID。
   */
  group_id?: string;
}

/**
 * @description 本地任务状态更新事件的负载接口。
 *              当 `LOCAL_TASK_STATE_UPDATED_EVENT` 事件被触发时，其 payload 应符合此结构。
 *              对应 Rust 结构体: `LocalTaskStateUpdatedEventPayload` (在 Rust 端的 `event.rs` 中定义)
 */
export interface LocalTaskStateUpdatedEventPayload {
  /**
   * @description 最新的任务调试状态对象 (`TaskDebugState`)。
   *              其具体结构定义在 `common_models` crate 中，并在 Rust 后端序列化为 JSON 后传递。
   *              前端接收时，此字段为该 JSON 对象。
   */
  new_state: TaskDebugState; // 在 TypeScript 中通常使用 any 或更具体的接口（如果为 TaskDebugState 创建了对应模型）
}

/**
 * @description Echo (回声) 响应事件的负载接口。
 *              当 `ECHO_RESPONSE_EVENT` 事件被触发时，其 payload 应符合此结构。
 *              对应 Rust 结构体: `EchoResponseEventPayload` (在 Rust 端的 `event.rs` 中定义)
 */
export interface EchoResponseEventPayload {
  /**
   * @description 从云端返回的 Echo 消息的具体内容。
   */
  content: string;
}

// TODO: 定义 TaskDebugState 接口，并用于 LocalTaskStateUpdatedEventPayload
// export interface TaskDebugState {
//   task_id: string;
//   current_step_id?: string;
//   overall_status?: string;
//   // ... 其他 TaskDebugState 中的字段
//   single_test_steps?: { [key: string]: SingleTestStepState };
//   pre_check_items?: { [key: string]: PreCheckItemState };
//   // ... 等等
// }

// export interface SingleTestStepState { ... }
// export interface PreCheckItemState { ... } 