<script setup lang="ts">
import type { ContainerInfo } from '@/types'
import { Pin } from '@lucide/vue'
import { SidebarMenuAction, SidebarMenuButton, SidebarMenuItem } from '@/components/ui/sidebar'
import { isRunning } from '@/types'

defineProps<{
  container: ContainerInfo
  active: boolean
  pinned: boolean
}>()
defineEmits<{
  select: [event: MouseEvent]
  togglePin: []
}>()
</script>

<template>
  <SidebarMenuItem>
    <SidebarMenuButton :is-active="active" @click="$emit('select', $event)">
      <span
        class="size-2 shrink-0 rounded-full"
        :class="isRunning(container) ? 'bg-green-500' : 'bg-muted-foreground'"
      />
      <span class="truncate">{{ container.name }}</span>
    </SidebarMenuButton>
    <SidebarMenuAction
      :show-on-hover="!pinned"
      :class="pinned ? 'text-foreground' : 'text-muted-foreground'"
      :title="pinned ? 'Unpin' : 'Pin'"
      @click="$emit('togglePin')"
    >
      <Pin :class="pinned ? 'fill-current' : ''" />
    </SidebarMenuAction>
  </SidebarMenuItem>
</template>
