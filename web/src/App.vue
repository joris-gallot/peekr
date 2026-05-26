<script setup lang="ts">
import type { Entry, LogStream } from '@/composables/useLogStream'
import type { JsonValueKind, LogLevel } from '@/lib/logfmt'
import { VisArea, VisXYContainer } from '@unovis/vue'
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, shallowRef, watch } from 'vue'
import { Badge } from '@/components/ui/badge'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { Input } from '@/components/ui/input'
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
import { Skeleton } from '@/components/ui/skeleton'
import { useLogStream } from '@/composables/useLogStream'
import { groupContainers } from '@/lib/group'
import { displayValue, formatTime, matchesFilter, parseFilter, valueKind } from '@/lib/logfmt'
import { mergeByTime } from '@/lib/merge'

interface ContainerInfo {
  id: string
  name: string
  image: string
  state: string
  status: string
  project: string
}

interface StatsSample {
  ts: number
  cpu_pct: number
  mem_used: number
  mem_limit: number
  mem_pct: number
}

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

// stable per-container accent colors for the merged timeline
const PALETTE = ['#38bdf8', '#a78bfa', '#34d399', '#fbbf24', '#f472b6', '#fb923c', '#22d3ee', '#a3e635']

const containers = ref<ContainerInfo[]>([])
const listError = ref('')
const filter = ref('')
const logFilter = ref('')
const logPane = ref<HTMLElement | null>(null)

const streams = shallowRef<LogStream[]>([])

const stats = ref<StatsSample | null>(null)
const cpuHistory = ref<number[]>([])
const memHistory = ref<number[]>([])
const STATS_POINTS = 60

const sidebarWidth = ref(localStorage.getItem('peekr.sidebarWidth') || '16rem')
const openGroups = reactive<Record<string, boolean>>(loadOpenGroups())

let statsSource: EventSource | null = null
let listTimer: ReturnType<typeof setInterval> | null = null
let stick = true

function loadOpenGroups(): Record<string, boolean> {
  try {
    return JSON.parse(localStorage.getItem('peekr.openGroups') || '{}')
  }
  catch {
    return {}
  }
}

const filteredContainers = computed(() =>
  containers.value.filter(c => c.name.toLowerCase().includes(filter.value.toLowerCase())),
)
const filterActive = computed(() => filter.value.trim().length > 0)
const groups = computed(() => groupContainers(filteredContainers.value))

const merged = computed(() => mergeByTime<Entry>(streams.value.map(s => s.entries.value)))

const visibleLogs = computed(() => {
  const terms = parseFilter(logFilter.value)
  return terms.length ? merged.value.filter(e => matchesFilter(e.log, terms)) : merged.value
})

watch(openGroups, v => localStorage.setItem('peekr.openGroups', JSON.stringify(v)))
watch(() => visibleLogs.value.length, () => {
  if (stick) {
    nextTick(() => {
      const el = logPane.value
      if (el)
        el.scrollTop = el.scrollHeight
    })
  }
})

function isRunning(c: ContainerInfo) {
  return c.state.toLowerCase().includes('running')
}
function isSelected(id: string) {
  return streams.value.some(s => s.id === id)
}
function containerById(id: string) {
  return containers.value.find(c => c.id === id)
}
function shortName(id: string) {
  return containerById(id)?.name ?? id.slice(0, 12)
}
function colorFor(id: string) {
  const i = streams.value.findIndex(s => s.id === id)
  return PALETTE[(i < 0 ? 0 : i) % PALETTE.length]
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
  if (e.log.level)
    return LEVEL_ACCENT[e.log.level]
  if (e.stream === 'stderr')
    return 'border-l-red-500/50'
  return 'border-l-transparent'
}
function msgClass(e: Entry) {
  if (e.stream === 'stderr' && !e.log.level)
    return 'text-red-300'
  return 'text-foreground/90'
}
function toggle(e: Entry) {
  if (e.log.json)
    e.log.expanded = !e.log.expanded
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

function addStream(id: string) {
  // reassign: shallowRef only reacts to .value replacement, not in-place push
  streams.value = [...streams.value, useLogStream(id)]
}
function removeStream(id: string) {
  const s = streams.value.find(s => s.id === id)
  if (!s)
    return
  s.close()
  streams.value = streams.value.filter(x => x.id !== id)
}
function closeAllStreams() {
  streams.value.forEach(s => s.close())
  streams.value = []
}

function select(c: ContainerInfo, ev: MouseEvent) {
  const additive = ev.metaKey || ev.ctrlKey
  if (additive) {
    isSelected(c.id) ? removeStream(c.id) : addStream(c.id)
  }
  else {
    if (streams.value.length === 1 && streams.value[0].id === c.id)
      return
    closeAllStreams()
    addStream(c.id)
  }
  stick = true
  syncStats()
}

function openStats(id: string) {
  statsSource?.close()
  statsSource = new EventSource(`/api/containers/${id}/stats`)
  statsSource.onmessage = (e) => {
    const s = JSON.parse(e.data) as StatsSample
    stats.value = s
    cpuHistory.value.push(s.cpu_pct)
    memHistory.value.push(s.mem_pct)
    if (cpuHistory.value.length > STATS_POINTS)
      cpuHistory.value.shift()
    if (memHistory.value.length > STATS_POINTS)
      memHistory.value.shift()
  }
}
function closeStats() {
  statsSource?.close()
  statsSource = null
}
// stats are single-container; only stream them when exactly one is selected
function syncStats() {
  closeStats()
  stats.value = null
  cpuHistory.value = []
  memHistory.value = []
  if (streams.value.length === 1)
    openStats(streams.value[0].id)
}

const sparkX = (_: number, i: number) => i
const sparkY = (v: number) => v

function formatBytes(n: number): string {
  if (n <= 0)
    return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), units.length - 1)
  return `${(n / 1024 ** i).toFixed(i ? 1 : 0)} ${units[i]}`
}

function onScroll() {
  const el = logPane.value
  if (!el)
    return
  stick = el.scrollHeight - el.scrollTop - el.clientHeight < 40
}

async function loadContainers() {
  try {
    const res = await fetch('/api/containers')
    if (!res.ok)
      throw new Error(`HTTP ${res.status}`)
    containers.value = await res.json()
    listError.value = ''
  }
  catch (e) {
    listError.value = e instanceof Error ? e.message : 'failed to load containers'
  }
}

const single = computed(() => (streams.value.length === 1 ? containerById(streams.value[0].id) : null))

onMounted(() => {
  loadContainers()
  listTimer = setInterval(loadContainers, 5000)
})
onBeforeUnmount(() => {
  closeAllStreams()
  closeStats()
  if (listTimer)
    clearInterval(listTimer)
})
</script>

<template>
  <SidebarProvider :width="sidebarWidth" class="h-screen">
    <Sidebar collapsible="none" class="border-r">
      <SidebarHeader class="gap-2 border-b">
        <div class="flex items-center gap-2">
          <span class="text-lg font-semibold tracking-tight">peekr</span>
          <Badge variant="secondary" class="ml-auto">
            {{ containers.length }}
          </Badge>
        </div>
        <Input v-model="filter" placeholder="Filter containers..." class="h-8" />
      </SidebarHeader>

      <SidebarContent class="gap-0">
        <p v-if="listError" class="px-3 py-2 text-xs text-red-500">
          {{ listError }}
        </p>

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
                    <SidebarMenuButton :is-active="isSelected(c.id)" @click="select(c, $event)">
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
      <template v-if="streams.length">
        <header class="flex min-h-13 items-center gap-3 border-b px-4 py-2">
          <template v-if="single">
            <span class="font-medium">{{ single.name }}</span>
            <Badge :variant="isRunning(single) ? 'default' : 'secondary'">
              {{ single.status }}
            </Badge>
            <span v-if="streams[0].conn.value === 'error'" class="flex items-center gap-1 text-xs text-red-400">
              <span class="size-1.5 animate-pulse rounded-full bg-red-500" />reconnecting
            </span>
            <span v-else-if="streams[0].conn.value === 'connecting'" class="text-xs text-muted-foreground">connecting</span>
            <span class="ml-auto truncate text-xs text-muted-foreground">{{ single.image }}</span>
          </template>
          <template v-else>
            <span class="shrink-0 text-xs font-medium uppercase tracking-wider text-muted-foreground">merged · {{ streams.length }}</span>
            <div class="flex flex-wrap items-center gap-1.5">
              <span
                v-for="s in streams"
                :key="s.id"
                class="group flex items-center gap-1 rounded px-1.5 py-0.5 text-xs"
                :style="{ color: colorFor(s.id), backgroundColor: `${colorFor(s.id)}1a` }"
              >
                <span
                  v-if="s.conn.value === 'error'"
                  class="size-1.5 animate-pulse rounded-full bg-current"
                />
                <span class="max-w-40 truncate">{{ shortName(s.id) }}</span>
                <button class="opacity-50 hover:opacity-100" @click="removeStream(s.id); syncStats()">✕</button>
              </span>
            </div>
          </template>
        </header>

        <template v-if="single">
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
        </template>

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
              <span class="shrink-0 select-none tabular-nums text-muted-foreground/40">{{ formatTime(e.ts) }}</span>
              <span
                v-if="streams.length > 1"
                class="max-w-32 shrink-0 select-none truncate rounded px-1 text-[10px] leading-5"
                :style="{ color: colorFor(e.cid), backgroundColor: `${colorFor(e.cid)}1a` }"
              >{{ shortName(e.cid) }}</span>
              <span v-if="e.log.json" class="w-3 shrink-0 select-none text-muted-foreground/40">{{ e.log.expanded ? '▾' : '▸' }}</span>
              <span
                v-if="e.log.level"
                class="shrink-0 select-none rounded px-1 text-[10px] font-semibold uppercase leading-5 tracking-wider"
                :class="chipClass(e.log.level)"
              >{{ e.log.level }}</span>
              <span class="min-w-0 flex-1 whitespace-pre-wrap break-all" :class="msgClass(e)">{{ e.log.message }}</span>
            </div>

            <div
              v-if="e.log.json && e.log.expanded"
              class="ml-18 mb-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 border-l border-border/50 py-1 pl-3"
            >
              <template v-for="(val, key) in e.log.json" :key="key">
                <span class="select-none text-sky-400/70">{{ key }}</span>
                <span class="break-all" :class="KIND_CLASS[valueKind(val)]">{{ displayValue(val) }}</span>
              </template>
            </div>
          </div>

          <p v-if="!visibleLogs.length && merged.length" class="px-3 py-4 text-muted-foreground">
            No lines match filter
          </p>
        </div>
      </template>

      <div v-else class="flex flex-1 flex-col items-center justify-center gap-1 text-sm text-muted-foreground">
        <span>Select a container to stream logs</span>
        <span class="text-xs">⌘/Ctrl-click to merge several</span>
      </div>
    </main>
  </SidebarProvider>
</template>
