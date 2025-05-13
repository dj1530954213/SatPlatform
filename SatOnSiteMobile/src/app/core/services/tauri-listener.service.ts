import { Injectable, NgZone } from '@angular/core';
import { BehaviorSubject, Subscription } from 'rxjs';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { 
  WS_CONNECTION_STATUS_EVENT,
  WS_REGISTRATION_STATUS_EVENT,
  WS_PARTNER_STATUS_EVENT,
  LOCAL_TASK_STATE_UPDATED_EVENT,
  ECHO_RESPONSE_EVENT
} from '../constants/tauri-events.constants';
import { 
  WsConnectionStatusEventPayload,
  WsRegistrationStatusEventPayload,
  // PartnerInfo, // PartnerInfo 已在此文件内定义
  LocalTaskStateUpdatedEventPayload,
  EchoResponseEventPayload
} from '../models/tauri-event-payloads.model';

/**
 * @description 伙伴信息接口，与 Rust 端 WsPartnerStatusEventPayload 及 app.component.ts 中的定义对应。
 *              用于从 Tauri 事件接收伙伴状态数据。
 */
export interface PartnerInfo {
  /**
   * @description 伙伴的角色 (例如："ControlCenter", "OnSiteMobile")
   */
  partner_role: string;
  /**
   * @description 伙伴是否在线
   */
  is_online: boolean;
  /**
   * @description （可选）伙伴的唯一客户端ID，由云端分配
   */
  partner_client_id?: string;
  /**
   * @description （可选）伙伴当前所在的任务组ID
   */
  group_id?: string;
}

/**
 * @description Tauri 事件监听服务。
 *              该服务封装了对 Rust 后端通过 Tauri 事件发送的各种状态更新的监听逻辑。
 *              它使用 RxJS BehaviorSubject 向应用的其组件暴露这些状态的 Observable 流。
 */
@Injectable({
  providedIn: 'root'
})
export class TauriListenerService {

  // --- BehaviorSubjects 用于管理和广播状态 --- 

  /**
   * @description WebSocket 连接状态的 BehaviorSubject。
   *              初始值为 null，当收到事件时更新。
   */
  private connectionStatusSubject = new BehaviorSubject<WsConnectionStatusEventPayload | null>(null);
  /**
   * @description WebSocket 连接状态的 Observable 流。
   *              组件可以订阅此 Observable 以获取连接状态的实时更新。
   */
  connectionStatus$ = this.connectionStatusSubject.asObservable();

  /**
   * @description 客户端注册状态的 BehaviorSubject。
   */
  private registrationStatusSubject = new BehaviorSubject<WsRegistrationStatusEventPayload | null>(null);
  /**
   * @description 客户端注册状态的 Observable 流。
   */
  registrationStatus$ = this.registrationStatusSubject.asObservable();

  /**
   * @description 伙伴客户端状态的 BehaviorSubject。
   */
  private partnerStatusSubject = new BehaviorSubject<PartnerInfo | null>(null);
  /**
   * @description 伙伴客户端状态的 Observable 流。
   */
  partnerStatus$ = this.partnerStatusSubject.asObservable();

  /**
   * @description 本地任务状态 (TaskDebugState) 的 BehaviorSubject。
   */
  private taskStateSubject = new BehaviorSubject<LocalTaskStateUpdatedEventPayload | null>(null);
  /**
   * @description 本地任务状态的 Observable 流。
   */
  taskState$ = this.taskStateSubject.asObservable();

  /**
   * @description Echo 响应的 BehaviorSubject。
   */
  private echoResponseSubject = new BehaviorSubject<EchoResponseEventPayload | null>(null);
  /**
   * @description Echo 响应的 Observable 流。
   */
  echoResponse$ = this.echoResponseSubject.asObservable();

  // --- Tauri 事件解绑函数数组 --- 
  /**
   * @description 存储所有 Tauri 事件监听器解绑函数的数组。
   *              在服务销毁时调用这些函数以避免内存泄漏。
   */
  private unlistenFns: UnlistenFn[] = [];

  /**
   * @description TauriListenerService 的构造函数。
   * @param ngZone Angular 的 NgZone 服务，用于确保 Tauri 事件回调在 Angular 的区域内执行，
   *               从而能够正确触发变更检测和UI更新。
   */
  constructor(private ngZone: NgZone) {
    console.log('[现场移动端] TauriListenerService 服务构造函数初始化');
  }

  /**
   * @description 初始化所有 Tauri 事件监听器。
   *              此方法会为每种关心的 Tauri 事件设置监听，并将收到的数据通过对应的 BehaviorSubject 发射出去。
   *              它会确保回调在 Angular Zone 内执行。
   */
  async initializeListeners(): Promise<void> {
    console.log('[现场移动端 TauriListenerService] 开始初始化事件监听器...');

    try {
      // 监听 WebSocket 连接状态事件
      const unlistenConnection = await listen<WsConnectionStatusEventPayload>(WS_CONNECTION_STATUS_EVENT, (event) => {
        console.log(`[现场移动端 TauriListenerService] 收到事件 ${WS_CONNECTION_STATUS_EVENT}:`, event.payload);
        this.ngZone.run(() => {
          this.connectionStatusSubject.next(event.payload);
        });
      });
      this.unlistenFns.push(unlistenConnection);

      // 监听客户端注册状态事件
      const unlistenRegistration = await listen<WsRegistrationStatusEventPayload>(WS_REGISTRATION_STATUS_EVENT, (event) => {
        console.log(`[现场移动端 TauriListenerService] 收到事件 ${WS_REGISTRATION_STATUS_EVENT}:`, event.payload);
        this.ngZone.run(() => {
          this.registrationStatusSubject.next(event.payload);
        });
      });
      this.unlistenFns.push(unlistenRegistration);

      // 监听伙伴状态更新事件
      const unlistenPartnerStatus = await listen<PartnerInfo>(WS_PARTNER_STATUS_EVENT, (event) => {
        console.log(`[现场移动端 TauriListenerService] 收到事件 ${WS_PARTNER_STATUS_EVENT}:`, event.payload);
        this.ngZone.run(() => {
          this.partnerStatusSubject.next(event.payload as PartnerInfo); // 确保类型正确
        });
      });
      this.unlistenFns.push(unlistenPartnerStatus);

      // 监听任务状态更新事件
      const unlistenTaskState = await listen<LocalTaskStateUpdatedEventPayload>(LOCAL_TASK_STATE_UPDATED_EVENT, (event) => {
        console.log(`[现场移动端 TauriListenerService] 收到事件 ${LOCAL_TASK_STATE_UPDATED_EVENT}:`, event.payload);
        this.ngZone.run(() => {
          this.taskStateSubject.next(event.payload);
        });
      });
      this.unlistenFns.push(unlistenTaskState);

      // 监听 Echo 响应事件
      const unlistenEcho = await listen<EchoResponseEventPayload>(ECHO_RESPONSE_EVENT, (event) => {
        console.log(`[现场移动端 TauriListenerService] 收到事件 ${ECHO_RESPONSE_EVENT}:`, event.payload);
        this.ngZone.run(() => {
          this.echoResponseSubject.next(event.payload);
        });
      });
      this.unlistenFns.push(unlistenEcho);

      console.log('[现场移动端 TauriListenerService] 所有事件监听器已成功初始化。');

    } catch (error) {
      console.error('[现场移动端 TauriListenerService] 初始化事件监听器时发生错误:', error);
    }
  }

  /**
   * @description 销毁所有 Tauri 事件监听器。
   *              此方法会遍历存储的解绑函数并逐个调用，以清理所有已设置的监听器。
   *              通常在应用或父组件销毁时调用，以防止内存泄漏。
   */
  destroyListeners(): void {
    console.log('[现场移动端 TauriListenerService] 开始销毁事件监听器...');
    this.unlistenFns.forEach(unlistenFn => unlistenFn());
    this.unlistenFns = []; // 清空数组
    console.log('[现场移动端 TauriListenerService] 所有事件监听器已成功销毁。');
  }
} 