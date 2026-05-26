<script setup lang="ts">
import type { ContainerInfo } from '@/types'
import { Moon, Pin, Sun } from '@lucide/vue'
import { useDark, useLocalStorage, useToggle } from '@vueuse/core'
import { computed, ref } from 'vue'
import ContainerRow from '@/components/ContainerRow.vue'
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
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'
import { groupContainers, partitionPinned } from '@/lib/group'

const props = defineProps<{
  containers: ContainerInfo[]
  listError: string
  selectedIds: string[]
}>()

const emit = defineEmits<{
  select: [container: ContainerInfo, event: MouseEvent]
}>()

const filter = ref('')
const openGroups = useLocalStorage<Record<string, boolean>>('peekr.openGroups', {})
const pins = useLocalStorage<string[]>('peekr.pins', [])

const isDark = useDark({ initialValue: 'dark' })
const toggleDark = useToggle(isDark)

const filterActive = computed(() => filter.value.trim().length > 0)
const filtered = computed(() =>
  props.containers.filter(c => c.name.toLowerCase().includes(filter.value.toLowerCase())),
)
const partition = computed(() => partitionPinned(filtered.value, pins.value))
const pinned = computed(() => partition.value.pinned)
const groups = computed(() => groupContainers(partition.value.rest))

function isSelected(id: string) {
  return props.selectedIds.includes(id)
}
function isPinned(name: string) {
  return pins.value.includes(name)
}
function togglePin(name: string) {
  pins.value = isPinned(name) ? pins.value.filter(n => n !== name) : [...pins.value, name]
}
// the unprojected bucket has an empty project; store its state under a readable key
function groupKey(project: string) {
  return project || 'ungrouped'
}
function isGroupOpen(project: string) {
  return filterActive.value ? true : openGroups.value[groupKey(project)] !== false
}
function setGroupOpen(project: string, open: boolean) {
  openGroups.value[groupKey(project)] = open
}
</script>

<template>
  <Sidebar collapsible="offcanvas" class="border-r">
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

      <SidebarGroup v-if="pinned.length" class="py-1">
        <SidebarGroupLabel class="gap-1.5">
          <Pin class="size-3 fill-current" />pinned
        </SidebarGroupLabel>
        <SidebarGroupContent>
          <SidebarMenu>
            <ContainerRow
              v-for="c in pinned"
              :key="c.id"
              :container="c"
              :active="isSelected(c.id)"
              :pinned="true"
              @select="emit('select', c, $event)"
              @toggle-pin="togglePin(c.name)"
            />
          </SidebarMenu>
        </SidebarGroupContent>
      </SidebarGroup>

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
                <ContainerRow
                  v-for="c in g.items"
                  :key="c.id"
                  :container="c"
                  :active="isSelected(c.id)"
                  :pinned="false"
                  @select="emit('select', c, $event)"
                  @toggle-pin="togglePin(c.name)"
                />
              </SidebarMenu>
            </SidebarGroupContent>
          </CollapsibleContent>
        </SidebarGroup>
      </Collapsible>

      <p v-if="!groups.length && !pinned.length && !listError" class="px-3 py-4 text-center text-xs text-muted-foreground">
        No containers
      </p>
    </SidebarContent>

    <SidebarFooter class="border-t">
      <SidebarMenu>
        <SidebarMenuItem>
          <SidebarMenuButton class="text-muted-foreground" @click="toggleDark()">
            <Sun v-if="isDark" />
            <Moon v-else />
            <span>{{ isDark ? 'Light mode' : 'Dark mode' }}</span>
          </SidebarMenuButton>
        </SidebarMenuItem>
      </SidebarMenu>
    </SidebarFooter>
  </Sidebar>
</template>
