import { Injectable, NgZone } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import { listen, Event as TauriEvent, UnlistenFn } from '@tauri-apps/api/event';
import { BehaviorSubject, Observable, Subject } from 'rxjs';
import {
    ClientRole,
    TaskDebugState,
    WsConnectionStatusEvent,
    WsRegistrationStatusEvent,
    WsPartnerStatusEvent,
    LocalTaskStateUpdatedEvent,
    EchoResponseEventPayload
} from '../models/tauri-events.model';

// 定义 Tauri 事件名称常量，与 Rust 端事件名称对应
const WS_CONNECTION_STATUS_EVENT = 'ws_connection_status';
const WS_REGISTRATION_STATUS_EVENT = 'ws_registration_status';
const WS_PARTNER_STATUS_EVENT = 'ws_partner_status';
const LOCAL_TASK_STATE_UPDATED_EVENT = 'local_task_state_updated';
const ECHO_RESPONSE_EVENT = 'echo_response_event'; // 假设事件名为 echo_response_event

/**
 * @service WebSocketService
 * @description
 * 负责管理与 Tauri 后端 WebSocket 服务的通信，包括连接、注册、
 * 监听状态事件以及暴露可观察的状态给 Angular 组件。
 */
@Injectable({
    providedIn: 'root'
})
export class WebSocketService {

    // --- 状态 BehaviorSubjects ---
    private readonly _connectionStatus$ = new BehaviorSubject<WsConnectionStatusEvent | null>(null);
    private readonly _registrationStatus$ = new BehaviorSubject<WsRegistrationStatusEvent | null>(null);
    private readonly _partnerStatus$ = new BehaviorSubject<WsPartnerStatusEvent | null>(null);
    private readonly _taskState$ = new BehaviorSubject<TaskDebugState | null>(null);
    private readonly _echoResponse$ = new Subject<EchoResponseEventPayload>(); // 使用 Subject 因为 Echo 通常是一次性事件

    // --- 可观察对象 (Observables) --- 
    public readonly connectionStatus$: Observable<WsConnectionStatusEvent | null> = this._connectionStatus$.asObservable();
    public readonly registrationStatus$: Observable<WsRegistrationStatusEvent | null> = this._registrationStatus$.asObservable();
    public readonly partnerStatus$: Observable<WsPartnerStatusEvent | null> = this._partnerStatus$.asObservable();
    public readonly taskState$: Observable<TaskDebugState | null> = this._taskState$.asObservable();
    public readonly echoResponse$: Observable<EchoResponseEventPayload> = this._echoResponse$.asObservable();

    private unlistenMap: Map<string, UnlistenFn> = new Map();

    constructor(private ngZone: NgZone) {
        this.listenToTauriEvents();
    }

    /**
     * 启动对所有相关 Tauri 后端事件的监听。
     */
    private async listenToTauriEvents(): Promise<void> {
        try {
            const connectionStatusUnlisten: UnlistenFn = await listen<
                WsConnectionStatusEvent
            >(WS_CONNECTION_STATUS_EVENT, (event: TauriEvent<WsConnectionStatusEvent>) => {
                this.ngZone.run(() => {
                    console.log('WebSocketService: Received WsConnectionStatusEvent', event.payload);
                    this._connectionStatus$.next(event.payload);
                });
            });
            this.unlistenMap.set(WS_CONNECTION_STATUS_EVENT, connectionStatusUnlisten);

            const registrationStatusUnlisten: UnlistenFn = await listen<
                WsRegistrationStatusEvent
            >(WS_REGISTRATION_STATUS_EVENT, (event: TauriEvent<WsRegistrationStatusEvent>) => {
                this.ngZone.run(() => {
                    console.log('WebSocketService: Received WsRegistrationStatusEvent', event.payload);
                    this._registrationStatus$.next(event.payload);
                });
            });
            this.unlistenMap.set(WS_REGISTRATION_STATUS_EVENT, registrationStatusUnlisten);

            const partnerStatusUnlisten: UnlistenFn = await listen<
                WsPartnerStatusEvent
            >(WS_PARTNER_STATUS_EVENT, (event: TauriEvent<WsPartnerStatusEvent>) => {
                this.ngZone.run(() => {
                    console.log('WebSocketService: Received WsPartnerStatusEvent', event.payload);
                    this._partnerStatus$.next(event.payload);
                });
            });
            this.unlistenMap.set(WS_PARTNER_STATUS_EVENT, partnerStatusUnlisten);

            const taskStateUpdatedUnlisten: UnlistenFn = await listen<
                LocalTaskStateUpdatedEvent
            >(LOCAL_TASK_STATE_UPDATED_EVENT, (event: TauriEvent<LocalTaskStateUpdatedEvent>) => {
                this.ngZone.run(() => {
                    console.log('WebSocketService: Received LocalTaskStateUpdatedEvent', event.payload);
                    // 确保 new_state 存在再更新
                    if (event.payload && event.payload.new_state) {
                      this._taskState$.next(event.payload.new_state);
                    } else {
                      console.warn('WebSocketService: Received LocalTaskStateUpdatedEvent with missing new_state', event);
                    }
                });
            });
            this.unlistenMap.set(LOCAL_TASK_STATE_UPDATED_EVENT, taskStateUpdatedUnlisten);

            const echoResponseUnlisten: UnlistenFn = await listen<
                EchoResponseEventPayload
            >(ECHO_RESPONSE_EVENT, (event: TauriEvent<EchoResponseEventPayload>) => {
                this.ngZone.run(() => {
                    console.log('WebSocketService: Received EchoResponseEvent', event.payload);
                    this._echoResponse$.next(event.payload);
                });
            });
            this.unlistenMap.set(ECHO_RESPONSE_EVENT, echoResponseUnlisten);

            console.log('WebSocketService: Successfully subscribed to Tauri backend events.');

        } catch (error) {
            console.error('WebSocketService: Error subscribing to Tauri backend events:', error);
            // 可以在这里向上抛出错误或通过某种方式通知应用监听失败
        }
    }

    /**
     * 调用 Tauri 命令连接到云端 WebSocket 服务。
     * @param url 要连接的云端 WebSocket URL。
     * @returns Promise，在命令调用完成后解析。
     */
    public async connectToCloud(url: string): Promise<void> {
        try {
            console.log(`WebSocketService: Attempting to connect to cloud via Tauri command with URL: ${url}`);
            // 调用 connect_to_cloud 命令
            await invoke<void>('connect_to_cloud', { url });
            console.log('WebSocketService: connect_to_cloud command invoked.');
        } catch (error) {
            console.error('WebSocketService: Error invoking connect_to_cloud command:', error);
            // 根据需要处理或重新抛出错误
            throw error;
        }
    }

    /**
     * 调用 Tauri 命令向云端注册客户端并关联任务。
     * @param groupId 要加入或创建的组的 ID。
     * @param taskId 要关联的调试任务的 ID。
     * @returns Promise，在命令调用完成后解析。
     */
    public async registerClientWithTask(groupId: string, taskId: string): Promise<void> {
        try {
            console.log(`WebSocketService: Attempting to register client with task via Tauri command. GroupID: ${groupId}, TaskID: ${taskId}`);
            // 调用 register_client_with_task 命令
            await invoke<void>('register_client_with_task', { groupId, taskId });
            console.log('WebSocketService: register_client_with_task command invoked.');
        } catch (error) {
            console.error('WebSocketService: Error invoking register_client_with_task command:', error);
            throw error;
        }
    }

    /**
     * 调用 Tauri 命令发送 Echo 测试消息。
     * @param content 要发送的 Echo 内容。
     * @returns Promise，在命令调用完成后解析。
     */
    public async sendEcho(content: string): Promise<void> {
        try {
            console.log(`WebSocketService: Attempting to send echo via Tauri command with content: ${content}`);
            // 调用 send_ws_echo 命令
            await invoke<void>('send_ws_echo', { content }); // 假设 Tauri 命令名为 send_ws_echo
            console.log('WebSocketService: send_ws_echo command invoked.');
        } catch (error) {
            console.error('WebSocketService: Error invoking send_ws_echo command:', error);
            throw error;
        }
    }

    /**
     * 在服务销毁时取消所有 Tauri 事件监听。
     */
    public ngOnDestroy(): void {
        console.log('WebSocketService: Cleaning up Tauri event listeners...');
        // 遍历 Map 并同步调用 unlisten 函数
        this.unlistenMap.forEach(unlistenFn => {
            try {
                unlistenFn(); // UnlistenFn 是同步函数
            } catch (e) {
                console.error('WebSocketService: Error calling unlisten function:', e);
            }
        });
        this.unlistenMap.clear();
        console.log('WebSocketService: Event listeners cleaned up.');

        // 完成 BehaviorSubjects 和 Subjects 以防止内存泄漏
        this._connectionStatus$.complete();
        this._registrationStatus$.complete();
        this._partnerStatus$.complete();
        this._taskState$.complete();
        this._echoResponse$.complete();
    }
} 