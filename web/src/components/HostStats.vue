<script setup lang="ts">
import type { HostStat } from '@/types'
import { VisArea, VisXYContainer } from '@unovis/vue'
import { onBeforeUnmount, ref, watch } from 'vue'

const props = defineProps<{ host: string }>()

const POINTS = 60
const stat = ref<HostStat | null>(null)
const cpu = ref<number[]>([])
const mem = ref<number[]>([])
const disk = ref<number[]>([])
const net = ref<number[]>([])

let source: EventSource | null = null

function reset() {
  stat.value = null
  cpu.value = []
  mem.value = []
  disk.value = []
  net.value = []
}

function push(arr: { value: number[] }, v: number) {
  arr.value.push(v)
  if (arr.value.length > POINTS)
    arr.value.shift()
}

function open() {
  source?.close()
  reset()
  source = new EventSource(`/api/hosts/${props.host}/stats`)
  source.onmessage = (e) => {
    const s = JSON.parse(e.data) as HostStat
    stat.value = s
    push(cpu, s.cpu_pct)
    push(mem, s.mem_pct)
    push(disk, s.disk_pct)
    push(net, s.net_rx + s.net_tx)
  }
}

watch(() => props.host, open, { immediate: true })
onBeforeUnmount(() => source?.close())

const sparkX = (_: number, i: number) => i
const sparkY = (v: number) => v

function bytes(n: number): string {
  if (n <= 0)
    return '0 B'
  const u = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), u.length - 1)
  return `${(n / 1024 ** i).toFixed(i ? 1 : 0)} ${u[i]}`
}
</script>

<template>
  <div class="grid grid-cols-2 gap-px border-b bg-border/60 lg:grid-cols-4">
    <!-- CPU -->
    <div class="relative h-20 overflow-hidden bg-background px-4 py-3">
      <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">CPU</span>
      <div class="font-mono text-2xl font-semibold tabular-nums text-sky-500 dark:text-sky-300">
        {{ stat ? stat.cpu_pct.toFixed(1) : '--' }}<span class="text-sm text-muted-foreground">%</span>
      </div>
      <div class="pointer-events-none absolute inset-x-0 bottom-0 h-8 opacity-70">
        <VisXYContainer :data="cpu" :height="32" :margin="{ top: 2, bottom: 0 }">
          <VisArea :x="sparkX" :y="sparkY" color="#0ea5e9" :opacity="0.25" />
        </VisXYContainer>
      </div>
    </div>

    <!-- Memory -->
    <div class="relative h-20 overflow-hidden bg-background px-4 py-3">
      <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">Memory</span>
      <div class="font-mono text-2xl font-semibold tabular-nums text-violet-500 dark:text-violet-300">
        {{ stat ? stat.mem_pct.toFixed(1) : '--' }}<span class="text-sm text-muted-foreground">%</span>
      </div>
      <div class="font-mono text-[11px] text-muted-foreground">
        {{ stat ? `${bytes(stat.mem_used)} / ${bytes(stat.mem_total)}` : '' }}
      </div>
      <div class="pointer-events-none absolute inset-x-0 bottom-0 h-8 opacity-70">
        <VisXYContainer :data="mem" :height="32" :margin="{ top: 2, bottom: 0 }">
          <VisArea :x="sparkX" :y="sparkY" color="#8b5cf6" :opacity="0.25" />
        </VisXYContainer>
      </div>
    </div>

    <!-- Disk -->
    <div class="relative h-20 overflow-hidden bg-background px-4 py-3">
      <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">Disk</span>
      <div class="font-mono text-2xl font-semibold tabular-nums text-amber-500 dark:text-amber-300">
        {{ stat ? stat.disk_pct.toFixed(1) : '--' }}<span class="text-sm text-muted-foreground">%</span>
      </div>
      <div class="font-mono text-[11px] text-muted-foreground">
        {{ stat ? `${bytes(stat.disk_used)} / ${bytes(stat.disk_total)}` : '' }}
      </div>
      <div class="pointer-events-none absolute inset-x-0 bottom-0 h-8 opacity-70">
        <VisXYContainer :data="disk" :height="32" :margin="{ top: 2, bottom: 0 }">
          <VisArea :x="sparkX" :y="sparkY" color="#f59e0b" :opacity="0.25" />
        </VisXYContainer>
      </div>
    </div>

    <!-- Network -->
    <div class="relative h-20 overflow-hidden bg-background px-4 py-3">
      <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">Network</span>
      <div class="font-mono text-sm font-semibold tabular-nums text-emerald-600 dark:text-emerald-300">
        <div>↓ {{ stat ? `${bytes(stat.net_rx)}/s` : '--' }}</div>
        <div>↑ {{ stat ? `${bytes(stat.net_tx)}/s` : '--' }}</div>
      </div>
      <div class="pointer-events-none absolute inset-x-0 bottom-0 h-8 opacity-70">
        <VisXYContainer :data="net" :height="32" :margin="{ top: 2, bottom: 0 }">
          <VisArea :x="sparkX" :y="sparkY" color="#10b981" :opacity="0.25" />
        </VisXYContainer>
      </div>
    </div>
  </div>
</template>
