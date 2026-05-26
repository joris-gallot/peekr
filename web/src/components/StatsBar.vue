<script setup lang="ts">
import type { StatsSample } from '@/types'
import { VisArea, VisXYContainer } from '@unovis/vue'
import { Skeleton } from '@/components/ui/skeleton'

defineProps<{
  stats: StatsSample | null
  cpu: number[]
  mem: number[]
}>()

const sparkX = (_: number, i: number) => i
const sparkY = (v: number) => v

function formatBytes(n: number): string {
  if (n <= 0)
    return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), units.length - 1)
  return `${(n / 1024 ** i).toFixed(i ? 1 : 0)} ${units[i]}`
}
</script>

<template>
  <div v-if="stats" class="grid grid-cols-2 gap-px border-b bg-border/60">
    <div class="relative h-13 overflow-hidden bg-background px-4 py-2">
      <div class="flex items-baseline gap-2">
        <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">CPU</span>
        <span class="font-mono text-lg font-semibold tabular-nums text-sky-300">
          {{ stats.cpu_pct.toFixed(1) }}<span class="text-xs text-muted-foreground">%</span>
        </span>
      </div>
      <div class="pointer-events-none absolute inset-x-0 bottom-0 h-10 opacity-80">
        <VisXYContainer :data="cpu" :height="40" :margin="{ top: 2, bottom: 0 }">
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
        <VisXYContainer :data="mem" :height="40" :margin="{ top: 2, bottom: 0 }">
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
