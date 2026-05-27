<script setup lang="ts">
import type { ContainerInfo } from '@/types'
import { LogOut, Monitor, Moon, Pin, Settings2, Sun } from '@lucide/vue'
import { useColorMode, useLocalStorage } from '@vueuse/core'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import ContainerRow from '@/components/ContainerRow.vue'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
} from '@/components/ui/sidebar'
import { useAuth } from '@/composables/useAuth'
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

// 'auto' (system) | 'light' | 'dark', default system
const mode = useColorMode()

const router = useRouter()
const { logout } = useAuth()
async function onLogout() {
  await logout()
  router.push('/login')
}

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
        <DropdownMenu>
          <DropdownMenuTrigger
            class="ml-auto flex size-7 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-foreground data-[state=open]:bg-accent"
          >
            <Settings2 class="size-4" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" class="w-40">
            <DropdownMenuLabel>Theme</DropdownMenuLabel>
            <DropdownMenuRadioGroup v-model="mode">
              <DropdownMenuRadioItem value="auto">
                <Monitor class="size-4" />System
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="light">
                <Sun class="size-4" />Light
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="dark">
                <Moon class="size-4" />Dark
              </DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
            <DropdownMenuSeparator />
            <DropdownMenuItem @click="onLogout">
              <LogOut class="size-4" />Sign out
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
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
  </Sidebar>
</template>
