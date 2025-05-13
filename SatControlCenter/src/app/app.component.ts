import { Component, OnInit, OnDestroy, ChangeDetectorRef } from '@angular/core';
import { RouterModule } from '@angular/router';
import { Observable, Subscription } from 'rxjs'; // 导入 Observable 和 Subscription
import { CommonModule } from '@angular/common'; // 导入 CommonModule 以使用 async pipe 和 ngIf/ngFor
import { FormsModule } from '@angular/forms'; // 导入 FormsModule 以使用 ngModel
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

// WebSocket 连接状态事件的 Payload (与 Rust 端 WsConnectionStatusEventPayload 对应)
interface WsConnectionStatusEventPayload {
  connected: boolean;
  client_id?: string | null;
  error_message?: string | null;
}

// 客户端注册状态事件的 Payload
interface WsRegistrationStatusEventPayload {
  success: boolean;
  message?: string | null;
  group_id?: string | null;
  task_id?: string | null;
  // 注意：根据 Rust 端 event.rs 的定义，这里可能还需要 assigned_client_id: Option<String>
  // 但是从 SatOnSiteMobile 侧来看，它监听的是这样的结构。需要确认 ControlCenter 的 ws_registration_status_event 载荷
}

// 伙伴客户端状态事件的 Payload
interface WsPartnerStatusEventPayload {
  partner_role: string; // 例如 "ControlCenter" 或 "OnSiteMobile"
  is_online: boolean;
  // 根据 Rust 端 event.rs, SatControlCenter 发送的 WsPartnerStatusEventPayload
  // 包含 partner_client_id?: string, group_id?: string，这里也需要同步
}

// Echo 响应事件的 Payload (与 Rust 端 EchoResponseEventPayload 对应)
interface EchoResponseEventPayload {
  content: string;
}

// 本地任务状态更新事件的 Payload (Rust 端发送 TaskDebugState)
interface LocalTaskStateUpdatedEventPayload {
  new_state: any; // 保持 any 或根据 Rust 端 common_models::TaskDebugState 调整
}

@Component({
  selector: 'app-root-control', // 修改 selector
  standalone: true,
  imports: [
    RouterModule, 
    CommonModule, // 添加 CommonModule
    FormsModule   // 添加 FormsModule
  ],
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.css']
})
export class AppComponent implements OnInit, OnDestroy {
  title = 'SatControlCenter';

  // --- WebSocket 相关属性 ---
  // 用于表单输入的属性
  wsUrl: string = 'ws://127.0.0.1:8088'; // 默认的 WebSocket URL
  groupId: string = 'test_group_01';
  taskId: string = 'test_task_001';

  // 用于UI绑定的属性
  connectionStatusMessage: string = '等待连接...';
  registrationStatusMessage: string = '尚未注册';
  partnerStatusMessage: string = '伙伴状态未知';
  taskStateString: string = '任务状态未初始化';

  // --- Echo 功能相关属性 ---
  echoInput: string = '你好，云端！来自 SatControlCenter。'; // 修改 echoInput 默认值
  echoResponseMessage: string = '等待 Echo 回复...';

  private unlistenConnectionStatus?: UnlistenFn;
  private unlistenRegistrationStatus?: UnlistenFn;
  private unlistenPartnerStatus?: UnlistenFn;
  private unlistenTaskStateUpdate?: UnlistenFn;
  private unlistenEcho?: UnlistenFn;

  /**
   * 构造函数，注入 WebSocketService。
   * @param webSocketService WebSocket 服务实例。
   */
  constructor(private cdr: ChangeDetectorRef) {
    // 移除从服务中获取 Observables 的代码
    // this.connectionStatus$ = this.webSocketService.connectionStatus$;
    // this.registrationStatus$ = this.webSocketService.registrationStatus$;
    // this.partnerStatus$ = this.webSocketService.partnerStatus$;
    // this.taskState$ = this.webSocketService.taskState$;
  }

  async ngOnInit(): Promise<void> {
    console.log('[SatControlCenter] AppComponent ngOnInit'); // 修改日志标识符

    // 监听 ws_connection_status 事件
    if (!this.unlistenConnectionStatus) {
      try {
        this.unlistenConnectionStatus = await listen<WsConnectionStatusEventPayload>('ws_connection_status', (event) => {
          console.log('[SatControlCenter] 接收到 Tauri 事件 "ws_connection_status":', event.payload); // 修改日志标识符
          this.connectionStatusMessage = event.payload.connected
            ? `已连接 (客户端ID: ${event.payload.client_id || '未知'})`
            : `已断开 (原因: ${event.payload.error_message || '未知'})`;
          this.cdr.detectChanges();
        });
        console.log('[SatControlCenter] 成功监听 "ws_connection_status" 事件。'); // 修改日志标识符
      } catch (error) {
        console.error('[SatControlCenter] 设置 "ws_connection_status" 监听器失败:', error); // 修改日志标识符
        this.connectionStatusMessage = '监听连接状态事件失败';
      }
    }

    // 监听 ws_registration_status_event 事件
    if (!this.unlistenRegistrationStatus) {
      try {
        // 注意：这里的 WsRegistrationStatusEventPayload 结构需要与 ControlCenter Rust 端发出的事件负载完全一致。
        // Rust event.rs 中定义为：WsRegistrationStatusEventPayload { success: bool, message: Option<String>, assigned_client_id: Option<String>, group_id: Option<String>, task_id: Option<String> }
        this.unlistenRegistrationStatus = await listen<{ success: boolean; message?: string | null; assigned_client_id?: string | null; group_id?: string | null; task_id?: string | null; }>('ws_registration_status_event', (event) => {
          console.log('[SatControlCenter] 接收到 Tauri 事件 "ws_registration_status_event":', event.payload); // 修改日志标识符
          if (event.payload.success) {
            this.registrationStatusMessage = `注册成功 (分配的客户端ID: ${event.payload.assigned_client_id || '未分配'}, 组: ${event.payload.group_id}, 任务: ${event.payload.task_id})`;
          } else {
            this.registrationStatusMessage = `注册失败: ${event.payload.message || '未知错误'}`;
          }
          this.cdr.detectChanges();
        });
        console.log('[SatControlCenter] 成功监听 "ws_registration_status_event" 事件。'); // 修改日志标识符
      } catch (error) {
        console.error('[SatControlCenter] 设置 "ws_registration_status_event" 监听器失败:', error); // 修改日志标识符
        this.registrationStatusMessage = '监听注册状态事件失败';
      }
    }
    
    // 监听 ws_partner_status_event 事件
    if (!this.unlistenPartnerStatus) {
      try {
        // Rust event.rs 中定义为：WsPartnerStatusEventPayload { partner_client_id: Option<String>, partner_role: String, group_id: Option<String>, is_online: bool }
        this.unlistenPartnerStatus = await listen<{ partner_client_id?: string | null; partner_role: string; group_id?: string | null; is_online: boolean; }>('ws_partner_status_event', (event) => {
          console.log('[SatControlCenter] 接收到 Tauri 事件 "ws_partner_status_event":', event.payload); // 修改日志标识符
          this.partnerStatusMessage = `伙伴 (${event.payload.partner_role}, ID: ${event.payload.partner_client_id || 'N/A'}) ${event.payload.is_online ? '已上线' : '已离线'} (组: ${event.payload.group_id || 'N/A'})`;
          this.cdr.detectChanges();
        });
        console.log('[SatControlCenter] 成功监听 "ws_partner_status_event" 事件。'); // 修改日志标识符
      } catch (error) {
        console.error('[SatControlCenter] 设置 "ws_partner_status_event" 监听器失败:', error); // 修改日志标识符
        this.partnerStatusMessage = '监听伙伴状态事件失败';
      }
    }

    // 监听 local_task_state_updated_event 事件
    if (!this.unlistenTaskStateUpdate) {
      try {
        this.unlistenTaskStateUpdate = await listen<LocalTaskStateUpdatedEventPayload>('local_task_state_updated_event', (event) => {
          console.log('[SatControlCenter] 接收到 Tauri 事件 "local_task_state_updated_event":', event.payload); // 修改日志标识符
          this.taskStateString = JSON.stringify(event.payload.new_state, null, 2);
          this.cdr.detectChanges();
        });
        console.log('[SatControlCenter] 成功监听 "local_task_state_updated_event" 事件。'); // 修改日志标识符
      } catch (error) {
        console.error('[SatControlCenter] 设置 "local_task_state_updated_event" 监听器失败:', error); // 修改日志标识符
        this.taskStateString = '监听任务状态更新事件失败';
      }
    }
    
    // 监听 echo_response_event 事件
    if (!this.unlistenEcho) {
      try {
        this.unlistenEcho = await listen<EchoResponseEventPayload>('echo_response_event', (event) => {
          console.log('[SatControlCenter] 接收到 Tauri 事件 "echo_response_event":', event.payload); // 修改日志标识符
          this.echoResponseMessage = `Echo 回复: ${event.payload.content}`;
          this.cdr.detectChanges();
        });
        console.log('[SatControlCenter] 成功监听 "echo_response_event" 事件。'); // 修改日志标识符
      } catch (error) {
        console.error('[SatControlCenter] 设置 "echo_response_event" 监听器失败:', error); // 修改日志标识符
        this.echoResponseMessage = '监听Echo事件失败';
      }
    }
  }

  /**
   * 组件销毁时取消所有订阅。
   */
  ngOnDestroy(): void {
    console.log('[SatControlCenter] AppComponent ngOnDestroy'); // 修改日志标识符
    if (this.unlistenConnectionStatus) {
      this.unlistenConnectionStatus();
    }
    if (this.unlistenRegistrationStatus) {
      this.unlistenRegistrationStatus();
    }
    if (this.unlistenPartnerStatus) {
      this.unlistenPartnerStatus();
    }
    if (this.unlistenTaskStateUpdate) {
      this.unlistenTaskStateUpdate();
    }
    if (this.unlistenEcho) {
      this.unlistenEcho();
    }
  }

  connectToCloud() {
    console.log('[SatControlCenter] 调用 Tauri 命令 connect_to_cloud'); // 修改日志标识符
    const connectionUrl = 'ws://127.0.0.1:8088'; 
    console.log(`[SatControlCenter] 尝试连接到: ${connectionUrl}`); // 修改日志标识符
    invoke('connect_to_cloud', { url: connectionUrl })
      .then(() => {
        console.log('[SatControlCenter] connect_to_cloud 命令调用成功'); // 修改日志标识符
      })
      .catch(error => {
        console.error('[SatControlCenter] connect_to_cloud 命令调用失败:', error); // 修改日志标识符
        this.connectionStatusMessage = `连接命令失败: ${error}`;
        this.cdr.detectChanges();
      });
  }

  // ControlCenter 通常不主动触发重连，此功能可能移除或保留观察
  triggerReconnect() {
    console.log('[SatControlCenter] 调用 Tauri 命令 trigger_reconnect (如果已实现)'); // 修改日志标识符
    invoke('trigger_reconnect')
      .then(() => console.log('[SatControlCenter] trigger_reconnect 命令调用成功')) // 修改日志标识符
      .catch(error => console.error('[SatControlCenter] trigger_reconnect 命令调用失败:', error)); // 修改日志标识符
  }

  // ControlCenter 通常不需要手动注册自己，它作为服务端角色或具有特殊注册逻辑。
  // 这个UI部分可以暂时保留，但逻辑可能需要适配 ControlCenter 的具体需求。
  // 例如，ControlCenter 可能只是查看或管理已注册的 OnSiteMobile 客户端。
  registerClient() {
    if (!this.groupId || !this.taskId) {
      this.registrationStatusMessage = '请输入组ID和任务ID (通常由现场端注册)';
      return;
    }
    console.log(`[SatControlCenter] 调用 Tauri 命令 register_client_with_task (通常由现场端执行), GroupID: ${this.groupId}, TaskID: ${this.taskId}`); // 修改日志标识符
    invoke('register_client_with_task', { groupId: this.groupId, taskId: this.taskId })
      .then(() => {
        // ControlCenter 侧的注册命令可能没有直接的前端状态反馈，或者反馈的是管理操作的结果
        this.registrationStatusMessage = '注册命令已发送 (具体状态请查看后端日志或特定事件)';
        console.log('[SatControlCenter] register_client_with_task 命令调用成功 (请确认ControlCenter是否应执行此操作)'); // 修改日志标识符
        this.cdr.detectChanges();
      })
      .catch(error => {
        console.error('[SatControlCenter] register_client_with_task 命令调用失败:', error); // 修改日志标识符
        this.registrationStatusMessage = `注册命令失败: ${error}`;
        this.cdr.detectChanges();
      });
  }

  sendEcho() {
    if (!this.echoInput) {
      this.echoResponseMessage = '请输入要发送的内容。';
      return;
    }
    console.log(`[SatControlCenter] 调用 Tauri 命令 send_ws_echo, 内容: "${this.echoInput}"`); // 修改日志标识符
    this.echoResponseMessage = 'Echo 已发送，等待回复...';
    invoke('send_ws_echo', { content: this.echoInput })
      .then(() => {
        console.log('[SatControlCenter] send_ws_echo 命令调用成功'); // 修改日志标识符
      })
      .catch(error => {
        console.error('[SatControlCenter] send_ws_echo 命令调用失败:', error); // 修改日志标识符
        this.echoResponseMessage = `发送 Echo 失败: ${error}`;
        this.cdr.detectChanges();
      });
  }
}
