import { createRouter, createWebHistory } from 'vue-router'
import { useAuth } from '@/composables/useAuth'
import LoginView from '@/views/LoginView.vue'
import LogsView from '@/views/LogsView.vue'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'logs', component: LogsView },
    { path: '/login', name: 'login', component: LoginView },
  ],
})

router.beforeEach(async (to) => {
  const { user, ready, refresh } = useAuth()
  if (!ready.value)
    await refresh()
  if (to.name !== 'login' && !user.value)
    return { name: 'login' }
  if (to.name === 'login' && user.value)
    return { name: 'logs' }
})
