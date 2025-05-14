import { Component } from '@angular/core';
import { DebugService, GenericResponse } from '../../../core/services/debug.service'; // 修正路径
import { FormsModule } from '@angular/forms'; // 导入 FormsModule
import { CommonModule } from '@angular/common'; // 导入 CommonModule for ngIf

@Component({
  selector: 'app-debug-sender',
  templateUrl: './debug-sender.component.html',
  styleUrls: ['./debug-sender.component.scss'],
  standalone: true, // 标记为独立组件
  imports: [FormsModule, CommonModule] // 导入 FormsModule 和 CommonModule
})
export class DebugSenderComponent {
  groupId: string = 'test_group_01'; // 默认 GroupID，与您测试时使用的保持一致
  debugNote: string = 'Test note from OnSiteMobile UI @ ' + new Date().toLocaleTimeString();
  isLoading: boolean = false;
  apiResponse: GenericResponse | null = null;

  constructor(private debugService: DebugService) {}

  async sendNote() {
    if (!this.groupId || !this.debugNote) {
      this.apiResponse = { success: false, message: 'Group ID and Note cannot be empty.' };
      return;
    }
    this.isLoading = true;
    this.apiResponse = null;
    try {
      this.apiResponse = await this.debugService.sendDebugNoteFromSite(this.groupId, this.debugNote);
    } catch (error) {
      // The service already catches and formats the error, but we can log it again if needed
      console.error("Error in component while sending note:", error);
      // Ensure apiResponse is set even if the service method itself throws an unexpected error
      // (though our current service implementation should always return a GenericResponse)
      if (!this.apiResponse) {
         const errorMessage = typeof error === 'string' ? error : 'Unknown component error.';
         this.apiResponse = { success: false, message: `Component level error: ${errorMessage}` };
      }
    }
    this.isLoading = false;
  }
} 