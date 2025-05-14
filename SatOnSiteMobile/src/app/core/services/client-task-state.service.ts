import { Injectable, OnDestroy } from '@angular/core';
import { BehaviorSubject, Observable, Subscription } from 'rxjs';
import { listen, Event as TauriEvent } from '@tauri-apps/api/event';

import { TaskDebugState } from '../models/task-debug-state.model';
import { LocalTaskStateUpdatedEventPayload } from '../models/tauri-event-payloads.model';

// 从 Rust 端 event.rs 获取准确的事件名称字符串
const LOCAL_TASK_STATE_UPDATED_EVENT_NAME = 'local_task_state_updated_event';

/**
 * @description Angular 服务，负责管理和提供客户端当前的 TaskDebugState。
 *              它会监听来自 Tauri Rust 后端的 LOCAL_TASK_STATE_UPDATED_EVENT 事件，
 *              并在接收到新的状态时更新一个 BehaviorSubject，供应用内其他组件订阅。
 */
@Injectable({
  providedIn: 'root',
})
export class ClientTaskStateService implements OnDestroy {
  private taskStateSubject: BehaviorSubject<TaskDebugState | null> =
    new BehaviorSubject<TaskDebugState | null>(null);

  /**
   * @description 可观察的当前任务调试状态。
   *              组件可以订阅此 Observable 来获取最新的 TaskDebugState。
   */
  public readonly currentTaskState$: Observable<TaskDebugState | null> =
    this.taskStateSubject.asObservable();

  private eventListenerUnlisten?: () => void; // 用于存储取消监听函数的变量
  private subscriptions: Subscription = new Subscription(); // 用于管理所有 RxJS 订阅

  constructor() {
    console.log('[ClientTaskStateService - OnSiteMobile] 服务正在初始化...'); // 为现场端添加标识
    this.initializeTauriListener();
  }

  private async initializeTauriListener(): Promise<void> {
    try {
      this.eventListenerUnlisten = await listen<LocalTaskStateUpdatedEventPayload>(
        LOCAL_TASK_STATE_UPDATED_EVENT_NAME,
        (event: TauriEvent<LocalTaskStateUpdatedEventPayload>) => {
          console.log(
            `[ClientTaskStateService - OnSiteMobile] 收到 '${LOCAL_TASK_STATE_UPDATED_EVENT_NAME}' 事件:`,
            event.payload
          );
          if (event.payload && event.payload.new_state) {
            const newState: TaskDebugState = event.payload.new_state;
            this.taskStateSubject.next(newState);
            console.log('[ClientTaskStateService - OnSiteMobile] TaskDebugState 已更新并通过 BehaviorSubject 发布。', newState);
          } else {
            console.warn(
              `[ClientTaskStateService - OnSiteMobile] 收到的 '${LOCAL_TASK_STATE_UPDATED_EVENT_NAME}' 事件 payload 或 new_state 字段无效。`, event.payload
            );
          }
        }
      );
      console.log(
        `[ClientTaskStateService - OnSiteMobile] 已成功订阅 Tauri 事件: '${LOCAL_TASK_STATE_UPDATED_EVENT_NAME}'`
      );
    } catch (error) {
      console.error(
        `[ClientTaskStateService - OnSiteMobile] 订阅 Tauri 事件 '${LOCAL_TASK_STATE_UPDATED_EVENT_NAME}' 失败:`,
        error
      );
    }
  }

  /**
   * @description 获取当前 TaskDebugState 的快照值。
   * @returns {TaskDebugState | null} 当前的状态，如果尚未接收到任何状态则为 null。
   */
  public getCurrentStateSnapshot(): TaskDebugState | null {
    return this.taskStateSubject.getValue();
  }

  ngOnDestroy(): void {
    console.log('[ClientTaskStateService - OnSiteMobile] 服务正在销毁，取消事件监听和订阅...');
    if (this.eventListenerUnlisten) {
      this.eventListenerUnlisten();
      console.log(`[ClientTaskStateService - OnSiteMobile] Tauri 事件 '${LOCAL_TASK_STATE_UPDATED_EVENT_NAME}' 的监听已取消。`);
    }
    this.subscriptions.unsubscribe();
    console.log('[ClientTaskStateService - OnSiteMobile] 所有内部 RxJS 订阅已取消。');
  }
} 