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

const containers = ref<ContainerInfo[]>([])
const selected = ref<ContainerInfo | null>(null)
const logs = ref<string[]>([])
const filter = ref('')
const logBottom = ref<HTMLElement | null>(null)
let source: EventSource | null = null

const filtered = computed(() =>
  containers.value.filter((c) =>
    c.name.toLowerCase().includes(filter.value.toLowerCase()),
  ),
)

function isRunning(c: ContainerInfo) {
  return c.state.toLowerCase().includes('running')
}

async function loadContainers() {
  const res = await fetch('/api/containers')
  containers.value = await res.json()
}

function select(c: ContainerInfo) {
  selected.value = c
  logs.value = []
  source?.close()
  source = new EventSource(`/api/containers/${c.id}/logs`)
  source.onmessage = (e) => {
    logs.value.push(e.data)
    if (logs.value.length > 2000) logs.value.shift()
    nextTick(() => logBottom.value?.scrollIntoView())
  }
}

onMounted(loadContainers)
onBeforeUnmount(() => source?.close())
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
        </nav>
      </ScrollArea>
    </aside>

    <main class="flex flex-1 flex-col overflow-hidden">
      <header v-if="selected" class="flex items-center gap-3 border-b px-4 py-3">
        <span class="font-medium">{{ selected.name }}</span>
        <Badge :variant="isRunning(selected) ? 'default' : 'secondary'">
          {{ selected.status }}
        </Badge>
        <span class="ml-auto truncate text-xs text-muted-foreground">{{ selected.image }}</span>
      </header>
      <ScrollArea v-if="selected" class="flex-1">
        <pre class="p-4 font-mono text-xs leading-relaxed"><span
          v-for="(line, i) in logs"
          :key="i"
          class="block whitespace-pre-wrap break-all"
        >{{ line }}</span><span ref="logBottom" /></pre>
      </ScrollArea>
      <div
        v-else
        class="flex flex-1 items-center justify-center text-sm text-muted-foreground"
      >
        Select a container to stream logs
      </div>
    </main>
  </div>
</template>
