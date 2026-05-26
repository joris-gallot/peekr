import { createRouter, createWebHistory } from 'vue-router'
import LogsView from '@/views/LogsView.vue'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'logs', component: LogsView },
  ],
})
