import { Routes } from '@angular/router';

export const routes: Routes = [
  {
    path: 'admin-state-injector',
    loadComponent: () => import('./cloud-admin-state-injector/cloud-admin-state-injector.component').then(m => m.CloudAdminStateInjectorComponent),
    title: 'Admin State Injector'
  },
  // You can add a default redirect to this new page or keep existing ones
  { path: '', redirectTo: '/admin-state-injector', pathMatch: 'full' },
  { path: '**', redirectTo: '/admin-state-injector' } // Fallback route
];
