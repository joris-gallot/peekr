<script setup lang="ts">
import type { CreatedHost } from '@/composables/useHosts'
import { Check, ChevronsUpDown, Copy, Plus, Server, Trash2 } from '@lucide/vue'
import { computed, ref } from 'vue'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import { useHosts } from '@/composables/useHosts'

const { hosts, activeHost, addHost, removeHost } = useHosts()

const current = computed(() => hosts.value.find(h => h.id === activeHost.value))

const addOpen = ref(false)
const name = ref('')
const error = ref('')
const busy = ref(false)
const created = ref<CreatedHost | null>(null)
const copied = ref(false)

const command = computed(() =>
  created.value
    ? `PEEKR_HUB=ws://${location.host} PEEKR_TOKEN=${created.value.token} peekr-agent`
    : '',
)

function openAdd() {
  name.value = ''
  error.value = ''
  created.value = null
  copied.value = false
  addOpen.value = true
}

async function create() {
  if (busy.value)
    return
  error.value = ''
  busy.value = true
  try {
    created.value = await addHost(name.value)
  }
  catch (e) {
    error.value = e instanceof Error ? e.message : 'failed'
  }
  finally {
    busy.value = false
  }
}

async function copy() {
  await navigator.clipboard.writeText(command.value)
  copied.value = true
  setTimeout(() => (copied.value = false), 1500)
}
</script>

<template>
  <DropdownMenu>
    <DropdownMenuTrigger
      class="flex w-full items-center gap-2 rounded-md border px-2 py-1.5 text-sm outline-none transition-colors hover:bg-accent data-[state=open]:bg-accent"
    >
      <Server class="size-3.5 shrink-0 text-muted-foreground" />
      <span class="truncate">{{ current?.name ?? activeHost }}</span>
      <span
        class="ml-auto size-1.5 shrink-0 rounded-full"
        :class="(current?.status ?? 'online') === 'online' ? 'bg-green-500' : 'bg-muted-foreground'"
      />
      <ChevronsUpDown class="size-3.5 shrink-0 text-muted-foreground" />
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" class="w-56">
      <DropdownMenuLabel>Hosts</DropdownMenuLabel>
      <DropdownMenuItem
        v-for="h in hosts"
        :key="h.id"
        class="gap-2"
        @click="activeHost = h.id"
      >
        <span
          class="size-1.5 shrink-0 rounded-full"
          :class="h.status === 'online' ? 'bg-green-500' : 'bg-muted-foreground'"
        />
        <span class="truncate">{{ h.name }}</span>
        <Check v-if="h.id === activeHost" class="ml-auto size-3.5" />
        <button
          v-else-if="h.id !== 'local'"
          class="ml-auto text-muted-foreground hover:text-red-500"
          title="Remove host"
          @click.stop="removeHost(h.id)"
        >
          <Trash2 class="size-3.5" />
        </button>
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem @click="openAdd">
        <Plus class="size-4" />Add host
      </DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>

  <Dialog v-model:open="addOpen">
    <DialogContent class="sm:max-w-md">
      <DialogHeader>
        <DialogTitle>Add a host</DialogTitle>
        <DialogDescription>Run a peekr agent on the remote machine to monitor it.</DialogDescription>
      </DialogHeader>

      <form v-if="!created" class="flex flex-col gap-3" @submit.prevent="create">
        <Input v-model="name" placeholder="Host name (e.g. vps-project)" autofocus />
        <p v-if="error" class="text-xs text-red-400">
          {{ error }}
        </p>
        <Button type="submit" :disabled="busy || !name.trim()">
          {{ busy ? 'Creating...' : 'Create' }}
        </Button>
      </form>

      <div v-else class="flex flex-col gap-3 text-sm">
        <p class="text-muted-foreground">
          Token (shown once) - run the agent on the host:
        </p>
        <div class="relative">
          <pre class="overflow-x-auto rounded-md border bg-muted/40 p-3 pr-10 font-mono text-xs">{{ command }}</pre>
          <button
            class="absolute right-2 top-2 text-muted-foreground hover:text-foreground"
            :title="copied ? 'Copied' : 'Copy'"
            @click="copy"
          >
            <Check v-if="copied" class="size-4 text-green-500" />
            <Copy v-else class="size-4" />
          </button>
        </div>
        <Button variant="secondary" @click="addOpen = false">
          Done
        </Button>
      </div>
    </DialogContent>
  </Dialog>
</template>
