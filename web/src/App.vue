<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'

interface ContainerInfo {
  id: string
  name: string
  image: string
  state: string
  status: string
}

interface LogLine {
  ts: string | null
  stream: 'stdout' | 'stderr'
  msg: string
}

type ConnState = 'idle' | 'connecting' | 'open' | 'error'

const containers = ref<ContainerInfo[]>([])
const listError = ref('')
const selected = ref<ContainerInfo | null>(null)
const logs = ref<LogLine[]>([])
const filter = ref('')
const conn = ref<ConnState>('idle')
const logPane = ref<HTMLElement | null>(null)

let source: EventSource | null = null
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let listTimer: ReturnType<typeof setInterval> | null = null
let lastTs: string | null = null
let stick = true

const filtered = computed(() =>
  containers.value.filter((c) =>
    c.name.toLowerCase().includes(filter.value.toLowerCase()),
  ),
)

function isRunning(c: ContainerInfo) {
  return c.state.toLowerCase().includes('running')
}

async function loadContainers() {
  try {
    const res = await fetch('/api/containers')
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    containers.value = await res.json()
    listError.value = ''
  } catch (e) {
    listError.value = e instanceof Error ? e.message : 'failed to load containers'
  }
}

function closeStream() {
  if (reconnectTimer) {
    clearTimeout(reconnectTimer)
    reconnectTimer = null
  }
  source?.close()
  source = null
}

function openStream(id: string) {
  closeStream()
  conn.value = 'connecting'
  // `since` resumes after a drop instead of re-dumping the tail (avoids dup lines)
  const since = lastTs ? Math.floor(Date.parse(lastTs) / 1000) : null
  const url = since
    ? `/api/containers/${id}/logs?since=${since}`
    : `/api/containers/${id}/logs`

  source = new EventSource(url)
  source.onopen = () => (conn.value = 'open')
  source.onmessage = (e) => appendLine(JSON.parse(e.data) as LogLine)
  source.addEventListener('stream-error', () => (conn.value = 'error'))
  source.onerror = () => {
    conn.value = 'error'
    // stop EventSource native retry (it drops `since`); resume ourselves
    closeStream()
    reconnectTimer = setTimeout(() => {
      if (selected.value?.id === id) openStream(id)
    }, 2000)
  }
}

function appendLine(line: LogLine) {
  // monotonic dedup: timestamps are ordered, so skip anything <= last seen
  if (line.ts && lastTs && line.ts <= lastTs) return
  if (line.ts) lastTs = line.ts

  logs.value.push(line)
  if (logs.value.length > 2000) logs.value.splice(0, logs.value.length - 2000)

  if (stick) {
    nextTick(() => {
      const el = logPane.value
      if (el) el.scrollTop = el.scrollHeight
    })
  }
}

function onScroll() {
  const el = logPane.value
  if (!el) return
  stick = el.scrollHeight - el.scrollTop - el.clientHeight < 40
}

function select(c: ContainerInfo) {
  selected.value = c
  logs.value = []
  lastTs = null
  stick = true
  openStream(c.id)
}

onMounted(() => {
  loadContainers()
  listTimer = setInterval(loadContainers, 5000)
})
onBeforeUnmount(() => {
  closeStream()
  if (listTimer) clearInterval(listTimer)
})
</script>

<template>
  <div class="flex h-screen bg-background text-foreground">
    <aside class="flex w-72 flex-col border-r">
      <div class="flex items-center gap-2 border-b px-4 py-3">
        <span class="text-lg font-semibold tracking-tight">peekr</span>
        <Badge variant="secondary" class="ml-auto">{{ containers.length }}</Badge>
      </div>
      <div class="p-2">
        <Input v-model="filter" placeholder="Filter containers..." class="h-8" />
      </div>
      <p v-if="listError" class="px-3 pb-2 text-xs text-red-500">{{ listError }}</p>
      <ScrollArea class="flex-1">
        <nav class="flex flex-col gap-0.5 p-2">
          <button
            v-for="c in filtered"
            :key="c.id"
            class="flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-accent"
            :class="{ 'bg-accent': selected?.id === c.id }"
            @click="select(c)"
          >
            <span
              class="size-2 shrink-0 rounded-full"
              :class="isRunning(c) ? 'bg-green-500' : 'bg-muted-foreground'"
            />
            <span class="truncate">{{ c.name }}</span>
          </button>
          <p
            v-if="!filtered.length && !listError"
            class="px-2 py-4 text-center text-xs text-muted-foreground"
          >
            No containers
          </p>
        </nav>
      </ScrollArea>
    </aside>

    <main class="flex flex-1 flex-col overflow-hidden">
      <header v-if="selected" class="flex items-center gap-3 border-b px-4 py-3">
        <span class="font-medium">{{ selected.name }}</span>
        <Badge :variant="isRunning(selected) ? 'default' : 'secondary'">
          {{ selected.status }}
        </Badge>
        <span
          v-if="conn === 'error'"
          class="flex items-center gap-1 text-xs text-red-500"
        >
          <span class="size-1.5 rounded-full bg-red-500" />reconnecting
        </span>
        <span
          v-else-if="conn === 'connecting'"
          class="text-xs text-muted-foreground"
        >connecting</span>
        <span class="ml-auto truncate text-xs text-muted-foreground">{{ selected.image }}</span>
      </header>
      <div
        v-if="selected"
        ref="logPane"
        class="flex-1 overflow-auto p-4 font-mono text-xs leading-relaxed"
        @scroll="onScroll"
      >
        <span
          v-for="(line, i) in logs"
          :key="i"
          class="block whitespace-pre-wrap break-all"
          :class="line.stream === 'stderr' ? 'text-red-400' : ''"
        >{{ line.msg }}</span>
      </div>
      <div
        v-else
        class="flex flex-1 items-center justify-center text-sm text-muted-foreground"
      >
        Select a container to stream logs
      </div>
    </main>
  </div>
</template>
