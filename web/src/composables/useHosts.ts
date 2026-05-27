import type { HostInfo } from '@/types'
import { ref } from 'vue'

export interface CreatedHost {
  id: string
  name: string
  token: string
}

// shared across the app: the known hosts and which one is active
const hosts = ref<HostInfo[]>([])
const activeHost = ref('local')

async function refresh() {
  try {
    const res = await fetch('/api/hosts')
    if (res.ok)
      hosts.value = await res.json()
  }
  catch {
    // keep last known list
  }
}

async function addHost(name: string): Promise<CreatedHost> {
  const res = await fetch('/api/hosts', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  })
  if (!res.ok)
    throw new Error((await res.text()) || 'failed to add host')
  const created: CreatedHost = await res.json()
  await refresh()
  return created
}

async function removeHost(id: string) {
  await fetch(`/api/hosts/${id}`, { method: 'DELETE' })
  if (activeHost.value === id)
    activeHost.value = 'local'
  await refresh()
}

export function useHosts() {
  return { hosts, activeHost, refresh, addHost, removeHost }
}
