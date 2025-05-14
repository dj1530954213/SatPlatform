import { Injectable } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';

export interface GenericResponse {
  success: boolean;
  message: string;
}

@Injectable({
  providedIn: 'root'
})
export class DebugService {

  constructor() { }

  async sendDebugNote(groupId: string, newNote: string, customSharedDataJsonString?: string): Promise<GenericResponse> {
    try {
      const response = await invoke<GenericResponse>('update_task_debug_note_cmd', {
        groupId,
        newNote,
        customSharedDataJsonString
      });
      console.log('Sent debug note (Control Center), response:', response);
      return response;
    } catch (error) {
      console.error('Error sending debug note (Control Center):', error);
      const errorMessage = typeof error === 'string' ? error : 'Unknown error sending debug note.';
      return { success: false, message: errorMessage };
    }
  }
} 