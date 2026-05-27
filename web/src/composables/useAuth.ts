import { ref } from 'vue'

export interface AuthUser {
  id: number
  email: string
  role: string
}

// module-level singleton: one auth state shared across the app
const user = ref<AuthUser | null>(null)
const ready = ref(false)

async function refresh() {
  try {
    const res = await fetch('/api/auth/me')
    user.value = res.ok ? await res.json() : null
  }
  catch {
    user.value = null
  }
  ready.value = true
}

async function post(path: string, body: unknown) {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok)
    throw new Error((await res.text()) || `${res.status}`)
  return res
}

async function login(email: string, password: string) {
  user.value = await (await post('/api/auth/login', { email, password })).json()
}

async function signup(email: string, password: string) {
  user.value = await (await post('/api/auth/signup', { email, password })).json()
}

async function logout() {
  await fetch('/api/auth/logout', { method: 'POST' })
  user.value = null
}

async function firstRun(): Promise<boolean> {
  try {
    const res = await fetch('/api/auth/first-run')
    return res.ok ? (await res.json()).firstRun : false
  }
  catch {
    return false
  }
}

export function useAuth() {
  return { user, ready, refresh, login, signup, logout, firstRun }
}
