import { Component, OnInit } from '@angular/core';
import { FormBuilder, FormGroup, Validators, ReactiveFormsModule } from '@angular/forms';
import { CommonModule } from '@angular/common';
import { invoke } from '@tauri-apps/api/core';

import { NzFormModule } from 'ng-zorro-antd/form';
import { NzInputModule } from 'ng-zorro-antd/input';
import { NzButtonModule } from 'ng-zorro-antd/button';
import { NzMessageService } from 'ng-zorro-antd/message';
import { NzGridModule } from 'ng-zorro-antd/grid';

@Component({
  selector: 'app-cloud-admin-state-injector',
  standalone: true,
  imports: [
    CommonModule,
    ReactiveFormsModule,
    NzFormModule,
    NzInputModule,
    NzButtonModule,
    NzGridModule
  ],
  templateUrl: './cloud-admin-state-injector.component.html',
  styleUrls: ['./cloud-admin-state-injector.component.css']
})
export class CloudAdminStateInjectorComponent implements OnInit {
  stateForm!: FormGroup;
  jsonPlaceholder: string = '例如：{"status_indicator": "SystemGreen"}';

  constructor(
    private fb: FormBuilder,
    private message: NzMessageService
  ) {}

  ngOnInit(): void {
    this.stateForm = this.fb.group({
      groupId: ['test_group_01', [Validators.required]],
      generalDebugNotes: ['云端主动推送：系统状态GREEN', [Validators.required]],
      customSharedDataJson: ['{"status_indicator": "SystemGreen"}', [this.jsonValidator, Validators.required]]
    });
  }

  // Custom validator for JSON string
  jsonValidator(control: any) {
    if (!control.value) {
      return null; // Allow empty
    }
    try {
      JSON.parse(control.value);
    } catch (e) {
      return { jsonInvalid: true };
    }
    return null;
  }

  async submitForm(): Promise<void> {
    if (this.stateForm.valid) {
      const { groupId, generalDebugNotes, customSharedDataJson } = this.stateForm.value;
      try {
        this.message.loading('Sending state update via Tauri invoke...', { nzDuration: 0 });
        // This is the Tauri command you will need to implement in your Rust backend
        await invoke('admin_broadcast_task_state_update_cmd', {
          groupId,
          debugNotes: generalDebugNotes,
          customDataJson: customSharedDataJson,
        });
        this.message.remove(); // Remove loading message
        this.message.success('State update command sent successfully via Tauri!');
        // Optionally reset form
        // this.stateForm.reset();
      } catch (error) {
        this.message.remove(); // Remove loading message
        console.error('Error invoking Tauri command:', error);
        this.message.error(`Error sending command: ${error}`);
      }
    } else {
      Object.values(this.stateForm.controls).forEach(control => {
        if (control.invalid) {
          control.markAsDirty();
          control.updateValueAndValidity({ onlySelf: true });
        }
      });
      this.message.error('Please correct the form errors.');
    }
  }
} 