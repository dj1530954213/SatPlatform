import { Component, OnInit, OnDestroy, ChangeDetectorRef } from '@angular/core';
import { FormsModule } from '@angular/forms'; // 保留 FormsModule，中心端也有
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
}

// 伙伴客户端状态事件的 Payload
interface WsPartnerStatusEventPayload {
  partner_role: string;
  is_online: boolean;
}

// Echo 响应事件的 Payload (与 Rust 端 EchoResponseEventPayload 对应)
interface EchoResponseEventPayload {
  content: string;
}

// 本地任务状态更新事件的 Payload (Rust 端发送 TaskDebugState)
interface LocalTaskStateUpdatedEventPayload {
  new_state: any; // 假设 new_state 是 TaskDebugState 的 JSON 结构，实际类型需要根据 common_models 定义
}

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [FormsModule], // 确保 FormsModule 在 imports 数组中
  templateUrl: './app.component.html',
  styleUrls: ['./app.component.css']
})
export class AppComponent implements OnInit, OnDestroy {
  private unlistenConnectionStatus?: UnlistenFn;
  private unlistenRegistrationStatus?: UnlistenFn;
  private unlistenPartnerStatus?: UnlistenFn;
  private unlistenTaskStateUpdate?: UnlistenFn;
  private unlistenEcho?: UnlistenFn;

  connectionStatusMessage: string = '等待连接...';
  registrationStatusMessage: string = '尚未注册';
  partnerStatusMessage: string = '伙伴状态未知';
  taskStateString: string = '任务状态未初始化';

  echoInput: string = '你好，云端！来自 SatOnSiteMobile。';
  echoResponseMessage: string = '等待 Echo 回复...';

  groupId: string = 'test_group_01';
  taskId: string = 'test_task_001';

  constructor(private cdr: ChangeDetectorRef) {
    console.log('[SatOnSiteMobile] AppComponent constructor');
  }

  async ngOnInit(): Promise<void> {
    console.log('[SatOnSiteMobile] AppComponent ngOnInit');

    if (!this.unlistenConnectionStatus) {
      try {
        this.unlistenConnectionStatus = await listen<WsConnectionStatusEventPayload>('ws_connection_status', (event) => {
          console.log('[SatOnSiteMobile] 接收到 Tauri 事件 "ws_connection_status":', event.payload);
          this.connectionStatusMessage = event.payload.connected
            ? `已连接 (客户端ID: ${event.payload.client_id || '未知'})`
            : `已断开 (原因: ${event.payload.error_message || '未知'})`;
          this.cdr.detectChanges();
        });
        console.log('[SatOnSiteMobile] 成功监听 "ws_connection_status" 事件。');
      } catch (error) {
        console.error('[SatOnSiteMobile] 设置 "ws_connection_status" 监听器失败:', error);
        this.connectionStatusMessage = '监听连接状态事件失败';
      }
    }

    if (!this.unlistenRegistrationStatus) {
      try {
        this.unlistenRegistrationStatus = await listen<WsRegistrationStatusEventPayload>('ws_registration_status_event', (event) => {
          console.log('[SatOnSiteMobile] 接收到 Tauri 事件 "ws_registration_status_event":', event.payload);
          this.registrationStatusMessage = event.payload.success
            ? `注册成功 (组: ${event.payload.group_id}, 任务: ${event.payload.task_id})`
            : `注册失败: ${event.payload.message || '未知错误'}`;
          this.cdr.detectChanges();
        });
        console.log('[SatOnSiteMobile] 成功监听 "ws_registration_status_event" 事件。');
      } catch (error) {
        console.error('[SatOnSiteMobile] 设置 "ws_registration_status_event" 监听器失败:', error);
        this.registrationStatusMessage = '监听注册状态事件失败';
      }
    }

    if (!this.unlistenPartnerStatus) {
      try {
        this.unlistenPartnerStatus = await listen<WsPartnerStatusEventPayload>('ws_partner_status_event', (event) => {
          console.log('[SatOnSiteMobile] 接收到 Tauri 事件 "ws_partner_status_event":', event.payload);
          this.partnerStatusMessage = `${event.payload.partner_role} ${event.payload.is_online ? '已上线' : '已离线'}`;
          this.cdr.detectChanges();
        });
        console.log('[SatOnSiteMobile] 成功监听 "ws_partner_status_event" 事件。');
      } catch (error) {
        console.error('[SatOnSiteMobile] 设置 "ws_partner_status_event" 监听器失败:', error);
        this.partnerStatusMessage = '监听伙伴状态事件失败';
      }
    }

    if (!this.unlistenTaskStateUpdate) {
      try {
        this.unlistenTaskStateUpdate = await listen<LocalTaskStateUpdatedEventPayload>('local_task_state_updated_event', (event) => {
          console.log('[SatOnSiteMobile] 接收到 Tauri 事件 "local_task_state_updated_event":', event.payload);
          this.taskStateString = JSON.stringify(event.payload.new_state, null, 2);
          this.cdr.detectChanges();
        });
        console.log('[SatOnSiteMobile] 成功监听 "local_task_state_updated_event" 事件。');
      } catch (error) {
        console.error('[SatOnSiteMobile] 设置 "local_task_state_updated_event" 监听器失败:', error);
        this.taskStateString = '监听任务状态更新事件失败';
      }
    }

    if (!this.unlistenEcho) {
      try {
        this.unlistenEcho = await listen<EchoResponseEventPayload>('echo_response_event', (event) => {
          console.log('[SatOnSiteMobile] 接收到 Tauri 事件 "echo_response_event":', event.payload);
          this.echoResponseMessage = `Echo 回复: ${event.payload.content}`;
          this.cdr.detectChanges();
        });
        console.log('[SatOnSiteMobile] 成功监听 "echo_response_event" 事件。');
      } catch (error) {
        console.error('[SatOnSiteMobile] 设置 "echo_response_event" 监听器失败:', error);
        this.echoResponseMessage = '监听Echo事件失败';
      }
    }
  }

  ngOnDestroy(): void {
    console.log('[SatOnSiteMobile] AppComponent ngOnDestroy');
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
    console.log('[SatOnSiteMobile] 调用 Tauri 命令 connect_to_cloud');
    const connectionUrl = 'ws://127.0.0.1:8088'; 
    console.log(`[SatOnSiteMobile] 尝试连接到: ${connectionUrl}`);
    invoke('connect_to_cloud', { url: connectionUrl })
      .then(() => {
        console.log('[SatOnSiteMobile] connect_to_cloud 命令调用成功');
      })
      .catch(error => {
        console.error('[SatOnSiteMobile] connect_to_cloud 命令调用失败:', error);
        this.connectionStatusMessage = `连接命令失败: ${error}`;
        this.cdr.detectChanges();
      });
  }

  triggerReconnect() {
    console.log('[SatOnSiteMobile] 调用 Tauri 命令 trigger_reconnect (如果已实现)');
    invoke('trigger_reconnect')
      .then(() => console.log('[SatOnSiteMobile] trigger_reconnect 命令调用成功'))
      .catch(error => console.error('[SatOnSiteMobile] trigger_reconnect 命令调用失败:', error));
  }

  registerClient() {
    if (!this.groupId || !this.taskId) {
      this.registrationStatusMessage = '请输入组ID和任务ID';
      return;
    }
    console.log(`[SatOnSiteMobile] 调用 Tauri 命令 register_client_with_task, GroupID: ${this.groupId}, TaskID: ${this.taskId}`);
    invoke('register_client_with_task', { groupId: this.groupId, taskId: this.taskId })
      .then(() => {
        this.registrationStatusMessage = '注册请求已发送...';
        console.log('[SatOnSiteMobile] register_client_with_task 命令调用成功');
        this.cdr.detectChanges();
      })
      .catch(error => {
        console.error('[SatOnSiteMobile] register_client_with_task 命令调用失败:', error);
        this.registrationStatusMessage = `注册命令失败: ${error}`;
        this.cdr.detectChanges();
      });
  }

  sendEcho() {
    if (!this.echoInput) {
      this.echoResponseMessage = '请输入要发送的内容。';
      return;
    }
    console.log(`[SatOnSiteMobile] 调用 Tauri 命令 send_ws_echo, 内容: "${this.echoInput}"`);
    this.echoResponseMessage = 'Echo 已发送，等待回复...';
    invoke('send_ws_echo', { content: this.echoInput })
      .then(() => {
        console.log('[SatOnSiteMobile] send_ws_echo 命令调用成功');
      })
      .catch(error => {
        console.error('[SatOnSiteMobile] send_ws_echo 命令调用失败:', error);
        this.echoResponseMessage = `发送 Echo 失败: ${error}`;
        this.cdr.detectChanges();
      });
  }
}
