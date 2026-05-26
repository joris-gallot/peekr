<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
  type JsonValueKind,
  type LogLevel,
  type ParsedLog,
  displayValue,
  formatTime,
  matchesFilter,
  parseFilter,
  parseLog,
  valueKind,
} from '@/lib/logfmt'

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

interface Entry {
  ts: string | null
  stream: 'stdout' | 'stderr'
  log: ParsedLog
}

type ConnState = 'idle' | 'connecting' | 'open' | 'error'

const LEVEL_CHIP: Record<LogLevel, string> = {
  trace: 'bg-zinc-500/15 text-zinc-400',
  debug: 'bg-zinc-500/20 text-zinc-300',
  info: 'bg-sky-500/15 text-sky-300',
  warn: 'bg-amber-500/15 text-amber-300',
  error: 'bg-red-500/15 text-red-300',
  fatal: 'bg-rose-600/25 text-rose-200',
}

const LEVEL_ACCENT: Record<LogLevel, string> = {
  trace: 'border-l-zinc-600/40',
  debug: 'border-l-zinc-500/40',
  info: 'border-l-sky-500/50',
  warn: 'border-l-amber-500/60',
  error: 'border-l-red-500/70',
  fatal: 'border-l-rose-500/80',
}

const KIND_CLASS: Record<JsonValueKind, string> = {
  string: 'text-emerald-300',
  number: 'text-amber-300',
  boolean: 'text-violet-300',
  null: 'text-zinc-500',
  complex: 'text-sky-300',
}

const containers = ref<ContainerInfo[]>([])
const listError = ref('')
const selected = ref<ContainerInfo | null>(null)
const entries = ref<Entry[]>([])
const filter = ref('')
const logFilter = ref('')
const conn = ref<ConnState>('idle')
const logPane = ref<HTMLElement | null>(null)

let source: EventSource | null = null
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let listTimer: ReturnType<typeof setInterval> | null = null
let lastTs: string | null = null
let stick = true

const filteredContainers = computed(() =>
  containers.value.filter((c) => c.name.toLowerCase().includes(filter.value.toLowerCase())),
)

const visibleLogs = computed(() => {
  const terms = parseFilter(logFilter.value)
  if (!terms.length) return entries.value
  return entries.value.filter((e) => matchesFilter(e.log, terms))
})

function isRunning(c: ContainerInfo) {
  return c.state.toLowerCase().includes('running')
}

function chipClass(level: LogLevel | null) {
  return level ? LEVEL_CHIP[level] : ''
}

function accentClass(e: Entry) {
  if (e.log.level) return LEVEL_ACCENT[e.log.level]
  if (e.stream === 'stderr') return 'border-l-red-500/50'
  return 'border-l-transparent'
}

function msgClass(e: Entry) {
  if (e.stream === 'stderr' && !e.log.level) return 'text-red-300'
  return 'text-foreground/90'
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
  const url = since ? `/api/containers/${id}/logs?since=${since}` : `/api/containers/${id}/logs`

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

  entries.value.push({ ts: line.ts, stream: line.stream, log: parseLog(line.msg) })
  if (entries.value.length > 2000) entries.value.splice(0, entries.value.length - 2000)

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

function toggle(e: Entry) {
  if (e.log.json) e.log.expanded = !e.log.expanded
}

function select(c: ContainerInfo) {
  selected.value = c
  entries.value = []
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
            v-for="c in filteredContainers"
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
            v-if="!filteredContainers.length && !listError"
            class="px-2 py-4 text-center text-xs text-muted-foreground"
          >
            No containers
          </p>
        </nav>
      </ScrollArea>
    </aside>

    <main class="flex flex-1 flex-col overflow-hidden">
      <template v-if="selected">
        <header class="flex items-center gap-3 border-b px-4 py-3">
          <span class="font-medium">{{ selected.name }}</span>
          <Badge :variant="isRunning(selected) ? 'default' : 'secondary'">
            {{ selected.status }}
          </Badge>
          <span v-if="conn === 'error'" class="flex items-center gap-1 text-xs text-red-400">
            <span class="size-1.5 animate-pulse rounded-full bg-red-500" />reconnecting
          </span>
          <span v-else-if="conn === 'connecting'" class="text-xs text-muted-foreground">
            connecting
          </span>
          <span class="ml-auto truncate text-xs text-muted-foreground">{{ selected.image }}</span>
        </header>

        <div class="border-b px-2 py-1.5">
          <Input
            v-model="logFilter"
            placeholder="Filter logs:  level=error   user.id=123   free text"
            class="h-8 font-mono text-xs"
          />
        </div>

        <div
          ref="logPane"
          class="flex-1 overflow-auto py-1 font-mono text-xs leading-relaxed"
          @scroll="onScroll"
        >
          <div v-for="(e, i) in visibleLogs" :key="i">
            <div
              class="flex gap-2 border-l-2 px-3 py-0.5 transition-colors hover:bg-muted/40"
              :class="[accentClass(e), e.log.json ? 'cursor-pointer' : '']"
              @click="toggle(e)"
            >
              <span class="shrink-0 select-none tabular-nums text-muted-foreground/40">
                {{ formatTime(e.ts) }}
              </span>
              <span
                v-if="e.log.json"
                class="w-3 shrink-0 select-none text-muted-foreground/40"
              >{{ e.log.expanded ? '▾' : '▸' }}</span>
              <span
                v-if="e.log.level"
                class="shrink-0 select-none rounded px-1 text-[10px] font-semibold uppercase leading-5 tracking-wider"
                :class="chipClass(e.log.level)"
              >{{ e.log.level }}</span>
              <span class="min-w-0 flex-1 whitespace-pre-wrap break-all" :class="msgClass(e)">{{
                e.log.message
              }}</span>
            </div>

            <div
              v-if="e.log.json && e.log.expanded"
              class="ml-18 mb-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 border-l border-border/50 py-1 pl-3"
            >
              <template v-for="(val, key) in e.log.json" :key="key">
                <span class="select-none text-sky-400/70">{{ key }}</span>
                <span class="break-all" :class="KIND_CLASS[valueKind(val)]">{{
                  displayValue(val)
                }}</span>
              </template>
            </div>
          </div>

          <p
            v-if="!visibleLogs.length && entries.length"
            class="px-3 py-4 text-muted-foreground"
          >
            No lines match filter
          </p>
        </div>
      </template>

      <div
        v-else
        class="flex flex-1 items-center justify-center text-sm text-muted-foreground"
      >
        Select a container to stream logs
      </div>
    </main>
  </div>
</template>
