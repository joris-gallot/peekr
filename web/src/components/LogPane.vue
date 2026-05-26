<script setup lang="ts">
import type { Entry } from '@/composables/useLogStream'
import type { JsonValueKind, LogLevel } from '@/lib/logfmt'
import { useScroll } from '@vueuse/core'
import { computed, nextTick, ref, watch } from 'vue'
import { displayValue, formatTime, matchesFilter, parseFilter, valueKind } from '@/lib/logfmt'

const props = withDefaults(defineProps<{
  entries: Entry[]
  filter?: string
  showSource?: boolean
  sourceName?: (cid: string) => string
  sourceColor?: (cid: string) => string
}>(), {
  filter: '',
  showSource: false,
})

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

const pane = ref<HTMLElement | null>(null)
// arrivedState.bottom stays true until the user scrolls up, so it doubles as the stick flag
const { arrivedState } = useScroll(pane, { offset: { bottom: 40 } })

const visible = computed(() => {
  const terms = parseFilter(props.filter)
  return terms.length ? props.entries.filter(e => matchesFilter(e.log, terms)) : props.entries
})

watch(() => visible.value.length, () => {
  if (arrivedState.bottom) {
    nextTick(() => {
      const el = pane.value
      if (el)
        el.scrollTop = el.scrollHeight
    })
  }
})

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
</script>

<template>
  <div
    ref="pane"
    class="flex-1 overflow-auto py-1 font-mono text-xs leading-relaxed"
  >
    <div v-for="(e, i) in visible" :key="i">
      <div
        class="flex gap-2 border-l-2 px-3 py-0.5 transition-colors hover:bg-muted/40"
        :class="[accentClass(e), e.log.json ? 'cursor-pointer' : '']"
        @click="toggle(e)"
      >
        <span class="shrink-0 select-none tabular-nums text-muted-foreground/40">{{ formatTime(e.ts) }}</span>
        <span
          v-if="showSource"
          class="max-w-32 shrink-0 select-none truncate rounded px-1 text-[10px] leading-5"
          :style="{ color: sourceColor?.(e.cid), backgroundColor: `${sourceColor?.(e.cid)}1a` }"
        >{{ sourceName?.(e.cid) }}</span>
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

    <p v-if="!visible.length && entries.length" class="px-3 py-4 text-muted-foreground">
      No lines match filter
    </p>
  </div>
</template>
