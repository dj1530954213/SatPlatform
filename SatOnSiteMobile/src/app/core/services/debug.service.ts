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

  async sendDebugNoteFromSite(groupId: string, newNote: string, customSharedDataJsonString?: string): Promise<GenericResponse> {
    try {
      const response = await invoke<GenericResponse>('send_debug_note_from_site_cmd', {
        groupId,
        newNote,
        customSharedDataJsonString
      });
      console.log('Sent debug note from site (with custom data if provided), response:', response);
      return response;
    } catch (error) {
      console.error('Error sending debug note from site:', error);
      const errorMessage = typeof error === 'string' ? error : 'Unknown error sending debug note.';
      return { success: false, message: errorMessage };
    }
  }
} 