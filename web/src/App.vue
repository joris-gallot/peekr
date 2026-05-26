<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
} from '@/components/ui/sidebar'
import { VisArea, VisXYContainer } from '@unovis/vue'
import { groupContainers } from '@/lib/group'
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
  project: string
}

interface LogLine {
  ts: string | null
  stream: 'stdout' | 'stderr'
  msg: string
}

interface StatsSample {
  ts: number
  cpu_pct: number
  mem_used: number
  mem_limit: number
  mem_pct: number
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

const stats = ref<StatsSample | null>(null)
const cpuHistory = ref<number[]>([])
const memHistory = ref<number[]>([])
const STATS_POINTS = 60

const sidebarWidth = ref(localStorage.getItem('peekr.sidebarWidth') || '16rem')
const openGroups = reactive<Record<string, boolean>>(loadOpenGroups())

let source: EventSource | null = null
let statsSource: EventSource | null = null
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let listTimer: ReturnType<typeof setInterval> | null = null
let lastTs: string | null = null
let stick = true

function loadOpenGroups(): Record<string, boolean> {
  try {
    return JSON.parse(localStorage.getItem('peekr.openGroups') || '{}')
  } catch {
    return {}
  }
}

const filteredContainers = computed(() =>
  containers.value.filter((c) => c.name.toLowerCase().includes(filter.value.toLowerCase())),
)
const filterActive = computed(() => filter.value.trim().length > 0)
const groups = computed(() => groupContainers(filteredContainers.value))

const visibleLogs = computed(() => {
  const terms = parseFilter(logFilter.value)
  if (!terms.length) return entries.value
  return entries.value.filter((e) => matchesFilter(e.log, terms))
})

watch(openGroups, (v) => localStorage.setItem('peekr.openGroups', JSON.stringify(v)))

function isRunning(c: ContainerInfo) {
  return c.state.toLowerCase().includes('running')
}

function isGroupOpen(project: string) {
  return filterActive.value ? true : openGroups[project] !== false
}

function setGroupOpen(project: string, open: boolean) {
  openGroups[project] = open
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

function startResize(e: PointerEvent) {
  e.preventDefault()
  const onMove = (ev: PointerEvent) => {
    sidebarWidth.value = `${Math.min(560, Math.max(200, ev.clientX))}px`
  }
  const onUp = () => {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    document.body.style.userSelect = ''
    localStorage.setItem('peekr.sidebarWidth', sidebarWidth.value)
  }
  document.body.style.userSelect = 'none'
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
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
  statsSource?.close()
  statsSource = null
}

function openStats(id: string) {
  statsSource?.close()
  // live metrics, no resume needed; native EventSource retry is fine
  statsSource = new EventSource(`/api/containers/${id}/stats`)
  statsSource.onmessage = (e) => {
    const s = JSON.parse(e.data) as StatsSample
    stats.value = s
    cpuHistory.value.push(s.cpu_pct)
    memHistory.value.push(s.mem_pct)
    if (cpuHistory.value.length > STATS_POINTS) cpuHistory.value.shift()
    if (memHistory.value.length > STATS_POINTS) memHistory.value.shift()
  }
}

const sparkX = (_: number, i: number) => i
const sparkY = (v: number) => v

function formatBytes(n: number): string {
  if (n <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), units.length - 1)
  return `${(n / 1024 ** i).toFixed(i ? 1 : 0)} ${units[i]}`
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
  stats.value = null
  cpuHistory.value = []
  memHistory.value = []
  openStream(c.id)
  openStats(c.id)
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
  <SidebarProvider :width="sidebarWidth" class="h-screen">
    <Sidebar collapsible="none" class="border-r">
      <SidebarHeader class="gap-2 border-b">
        <div class="flex items-center gap-2">
          <span class="text-lg font-semibold tracking-tight">peekr</span>
          <Badge variant="secondary" class="ml-auto">{{ containers.length }}</Badge>
        </div>
        <Input v-model="filter" placeholder="Filter containers..." class="h-8" />
      </SidebarHeader>

      <SidebarContent class="gap-0">
        <p v-if="listError" class="px-3 py-2 text-xs text-red-500">{{ listError }}</p>

        <Collapsible
          v-for="g in groups"
          :key="g.project"
          :open="isGroupOpen(g.project)"
          @update:open="setGroupOpen(g.project, $event)"
        >
          <SidebarGroup class="py-1">
            <SidebarGroupLabel as-child>
              <CollapsibleTrigger class="flex w-full items-center gap-1.5 hover:text-foreground">
                <span
                  class="text-muted-foreground/50 transition-transform duration-150"
                  :class="isGroupOpen(g.project) ? 'rotate-90' : ''"
                >▸</span>
                <span class="truncate">{{ g.project || 'ungrouped' }}</span>
                <span class="ml-auto tabular-nums text-muted-foreground/60">{{ g.items.length }}</span>
              </CollapsibleTrigger>
            </SidebarGroupLabel>
            <CollapsibleContent>
              <SidebarGroupContent>
                <SidebarMenu>
                  <SidebarMenuItem v-for="c in g.items" :key="c.id">
                    <SidebarMenuButton :is-active="selected?.id === c.id" @click="select(c)">
                      <span
                        class="size-2 shrink-0 rounded-full"
                        :class="isRunning(c) ? 'bg-green-500' : 'bg-muted-foreground'"
                      />
                      <span class="truncate">{{ c.name }}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </CollapsibleContent>
          </SidebarGroup>
        </Collapsible>

        <p v-if="!groups.length && !listError" class="px-3 py-4 text-center text-xs text-muted-foreground">
          No containers
        </p>
      </SidebarContent>
    </Sidebar>

    <div
      class="w-1 shrink-0 cursor-col-resize bg-border/40 transition-colors hover:bg-primary/60"
      @pointerdown="startResize"
    />

    <main class="flex min-w-0 flex-1 flex-col overflow-hidden bg-background">
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

        <div v-if="stats" class="grid grid-cols-2 gap-px border-b bg-border/60">
          <div class="relative h-13 overflow-hidden bg-background px-4 py-2">
            <div class="flex items-baseline gap-2">
              <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">CPU</span>
              <span class="font-mono text-lg font-semibold tabular-nums text-sky-300">
                {{ stats.cpu_pct.toFixed(1) }}<span class="text-xs text-muted-foreground">%</span>
              </span>
            </div>
            <div class="pointer-events-none absolute inset-x-0 bottom-0 h-10 opacity-80">
              <VisXYContainer :data="cpuHistory" :height="40" :margin="{ top: 2, bottom: 0 }">
                <VisArea :x="sparkX" :y="sparkY" color="#38bdf8" :opacity="0.25" />
              </VisXYContainer>
            </div>
          </div>
          <div class="relative h-13 overflow-hidden bg-background px-4 py-2">
            <div class="flex items-baseline gap-2">
              <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">MEM</span>
              <span class="font-mono text-lg font-semibold tabular-nums text-violet-300">
                {{ stats.mem_pct.toFixed(1) }}<span class="text-xs text-muted-foreground">%</span>
              </span>
              <span class="ml-auto self-center font-mono text-xs text-muted-foreground">
                {{ formatBytes(stats.mem_used) }} / {{ formatBytes(stats.mem_limit) }}
              </span>
            </div>
            <div class="pointer-events-none absolute inset-x-0 bottom-0 h-10 opacity-80">
              <VisXYContainer :data="memHistory" :height="40" :margin="{ top: 2, bottom: 0 }">
                <VisArea :x="sparkX" :y="sparkY" color="#a78bfa" :opacity="0.25" />
              </VisXYContainer>
            </div>
          </div>
        </div>
        <div v-else class="grid grid-cols-2 gap-px border-b bg-border/60">
          <div v-for="n in 2" :key="n" class="flex h-13 items-center gap-2 bg-background px-4">
            <Skeleton class="h-3 w-8" />
            <Skeleton class="h-5 w-14" />
          </div>
        </div>

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

          <p v-if="!visibleLogs.length && entries.length" class="px-3 py-4 text-muted-foreground">
            No lines match filter
          </p>
        </div>
      </template>

      <div v-else class="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        Select a container to stream logs
      </div>
    </main>
  </SidebarProvider>
</template>
