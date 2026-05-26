<script setup lang="ts">
import type { ContainerInfo } from '@/types'
import { computed, reactive, ref, watch } from 'vue'
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
} from '@/components/ui/sidebar'
import { groupContainers } from '@/lib/group'
import { isRunning } from '@/types'

const props = defineProps<{
  containers: ContainerInfo[]
  listError: string
  selectedIds: string[]
}>()

const emit = defineEmits<{
  select: [container: ContainerInfo, event: MouseEvent]
}>()

const filter = ref('')
const openGroups = reactive<Record<string, boolean>>(loadOpenGroups())

function loadOpenGroups(): Record<string, boolean> {
  try {
    return JSON.parse(localStorage.getItem('peekr.openGroups') || '{}')
  }
  catch {
    return {}
  }
}
watch(openGroups, v => localStorage.setItem('peekr.openGroups', JSON.stringify(v)))

const filterActive = computed(() => filter.value.trim().length > 0)
const groups = computed(() =>
  groupContainers(
    props.containers.filter(c => c.name.toLowerCase().includes(filter.value.toLowerCase())),
  ),
)

function isSelected(id: string) {
  return props.selectedIds.includes(id)
}
function isGroupOpen(project: string) {
  return filterActive.value ? true : openGroups[project] !== false
}
function setGroupOpen(project: string, open: boolean) {
  openGroups[project] = open
}
</script>

<template>
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
                  <SidebarMenuButton :is-active="isSelected(c.id)" @click="emit('select', c, $event)">
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
</template>
