<script setup lang="ts">
import type { LogStream } from '@/composables/useLogStream'
import type { ContainerInfo, StatsSample } from '@/types'
import { Columns2, Rows3 } from '@lucide/vue'
import { computed, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue'
import AppSidebar from '@/components/AppSidebar.vue'
import LogPane from '@/components/LogPane.vue'
import StatsBar from '@/components/StatsBar.vue'
import { Badge } from '@/components/ui/badge'
import { SidebarProvider } from '@/components/ui/sidebar'
import { useLogStream } from '@/composables/useLogStream'
import { mergeByTime } from '@/lib/merge'
import { isRunning } from '@/types'

// stable per-container accent colors for the merged timeline / split headers
const PALETTE = ['#38bdf8', '#a78bfa', '#34d399', '#fbbf24', '#f472b6', '#fb923c', '#22d3ee', '#a3e635']

const containers = ref<ContainerInfo[]>([])
const listError = ref('')
const logFilter = ref('')
const streams = shallowRef<LogStream[]>([])
const viewMode = ref<'merged' | 'split'>((localStorage.getItem('peekr.viewMode') as 'merged' | 'split') || 'merged')

const stats = ref<StatsSample | null>(null)
const cpuHistory = ref<number[]>([])
const memHistory = ref<number[]>([])
const STATS_POINTS = 60

const sidebarWidth = ref(localStorage.getItem('peekr.sidebarWidth') || '16rem')

let statsSource: EventSource | null = null
let listTimer: ReturnType<typeof setInterval> | null = null

const selectedIds = computed(() => streams.value.map(s => s.id))
const single = computed(() => (streams.value.length === 1 ? containerById(streams.value[0].id) : null))
const merged = computed(() => mergeByTime(streams.value.map(s => s.entries.value)))

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

function setView(mode: 'merged' | 'split') {
  viewMode.value = mode
  localStorage.setItem('peekr.viewMode', mode)
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
  syncStats()
}
function isSelected(id: string) {
  return streams.value.some(s => s.id === id)
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
    if (!res.ok)
      throw new Error(`HTTP ${res.status}`)
    containers.value = await res.json()
    listError.value = ''
  }
  catch (e) {
    listError.value = e instanceof Error ? e.message : 'failed to load containers'
  }
}

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
    <AppSidebar
      :containers="containers"
      :list-error="listError"
      :selected-ids="selectedIds"
      @select="select"
    />

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
            <div class="flex flex-wrap items-center gap-1.5">
              <span
                v-for="s in streams"
                :key="s.id"
                class="flex items-center gap-1 rounded px-1.5 py-0.5 text-xs"
                :style="{ color: colorFor(s.id), backgroundColor: `${colorFor(s.id)}1a` }"
              >
                <span v-if="s.conn.value === 'error'" class="size-1.5 animate-pulse rounded-full bg-current" />
                <span class="max-w-40 truncate">{{ shortName(s.id) }}</span>
                <button class="opacity-50 hover:opacity-100" @click="removeStream(s.id); syncStats()">✕</button>
              </span>
            </div>
            <div class="ml-auto flex shrink-0 overflow-hidden rounded-md border text-xs">
              <button
                class="flex items-center gap-1.5 px-2 py-1 transition-colors"
                :class="viewMode === 'merged' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/50'"
                title="Merged timeline"
                @click="setView('merged')"
              >
                <Rows3 :size="13" />merged
              </button>
              <button
                class="flex items-center gap-1.5 border-l px-2 py-1 transition-colors"
                :class="viewMode === 'split' ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/50'"
                title="Side-by-side panes"
                @click="setView('split')"
              >
                <Columns2 :size="13" />split
              </button>
            </div>
          </template>
        </header>

        <StatsBar v-if="single" :stats="stats" :cpu="cpuHistory" :mem="memHistory" />

        <div class="border-b px-2 py-1.5">
          <input
            v-model="logFilter"
            placeholder="Filter logs:  level=error   user.id=123   free text"
            class="h-8 w-full rounded-md border bg-transparent px-3 font-mono text-xs outline-none focus-visible:ring-1 focus-visible:ring-ring"
          >
        </div>

        <LogPane
          v-if="single || viewMode === 'merged'"
          :entries="merged"
          :filter="logFilter"
          :show-source="streams.length > 1"
          :source-name="shortName"
          :source-color="colorFor"
        />
        <div v-else class="flex min-h-0 flex-1">
          <div
            v-for="s in streams"
            :key="s.id"
            class="flex min-w-0 flex-1 flex-col border-l first:border-l-0"
          >
            <div class="flex items-center gap-1.5 border-b px-3 py-1.5 text-xs">
              <span class="size-2 shrink-0 rounded-full" :style="{ backgroundColor: colorFor(s.id) }" />
              <span class="truncate font-medium">{{ shortName(s.id) }}</span>
              <span v-if="s.conn.value === 'error'" class="ml-auto size-1.5 shrink-0 animate-pulse rounded-full bg-red-500" />
            </div>
            <LogPane :entries="s.entries.value" :filter="logFilter" />
          </div>
        </div>
      </template>

      <div v-else class="flex flex-1 flex-col items-center justify-center gap-1 text-sm text-muted-foreground">
        <span>Select a container to stream logs</span>
        <span class="text-xs">⌘/Ctrl-click to merge several</span>
      </div>
    </main>
  </SidebarProvider>
</template>
