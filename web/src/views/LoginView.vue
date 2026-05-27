<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useAuth } from '@/composables/useAuth'

const router = useRouter()
const { login, signup, firstRun } = useAuth()

const isFirstRun = ref(false)
const email = ref('')
const password = ref('')
const confirm = ref('')
const error = ref('')
const busy = ref(false)

// faint scrolling log backdrop - "peeking" into a stream
const backdrop = [
  '08:01:03.221Z  INFO   request handled  path=/api/items  status=200',
  '08:01:03.244Z  DEBUG  cache hit  key=user:42  ttl=58s',
  '08:01:03.402Z  WARN   slow query  120ms  SELECT * FROM events',
  '08:01:04.118Z  INFO   container started  web-1  image=nginx:1.27',
  '08:01:04.556Z  ERROR  db timeout  context deadline exceeded',
  '08:01:05.010Z  INFO   GET /healthz  200  1ms',
  '08:01:05.331Z  DEBUG  gc pause  0.4ms  heap=128MB',
  '08:01:06.002Z  INFO   request handled  path=/api/stats  status=200',
  '08:01:06.487Z  WARN   retry  attempt=2  upstream=api',
  '08:01:07.090Z  INFO   websocket connected  agent=vps-fra-1',
  '08:01:07.640Z  DEBUG  flush metrics  cpu=3.2%  mem=41%',
  '08:01:08.213Z  ERROR  connection reset  peer=10.0.0.4',
]

onMounted(async () => {
  isFirstRun.value = await firstRun()
})

async function submit() {
  if (busy.value)
    return
  error.value = ''
  if (isFirstRun.value && password.value !== confirm.value) {
    error.value = 'Passwords do not match'
    return
  }
  busy.value = true
  try {
    if (isFirstRun.value)
      await signup(email.value, password.value)
    else
      await login(email.value, password.value)
    router.push('/')
  }
  catch (e) {
    error.value = e instanceof Error ? e.message : 'Authentication failed'
  }
  finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="relative flex min-h-screen items-center justify-center overflow-hidden bg-background px-6">
    <!-- streaming-log backdrop -->
    <div class="logwall pointer-events-none absolute inset-0 select-none font-mono text-xs text-muted-foreground/[0.07]">
      <div class="logwall-track">
        <div v-for="(line, i) in [...backdrop, ...backdrop]" :key="i" class="whitespace-pre px-6 leading-6">
          {{ line }}
        </div>
      </div>
    </div>
    <!-- accent glow -->
    <div class="glow pointer-events-none absolute inset-0" />

    <div class="card relative z-10 w-full max-w-sm rounded-xl border bg-card/70 p-8 shadow-2xl backdrop-blur-sm">
      <div class="mb-6 flex items-center gap-2">
        <span class="text-2xl font-semibold tracking-tight">peekr</span>
        <span class="relative flex size-2">
          <span class="absolute inline-flex size-full animate-ping rounded-full bg-emerald-500/70" />
          <span class="relative inline-flex size-2 rounded-full bg-emerald-500" />
        </span>
        <span class="ml-auto font-mono text-[10px] uppercase tracking-widest text-muted-foreground">
          docker observability
        </span>
      </div>

      <h1 class="text-lg font-medium">
        {{ isFirstRun ? 'Create the owner account' : 'Sign in' }}
      </h1>
      <p class="mt-1 mb-6 text-xs text-muted-foreground">
        {{ isFirstRun ? 'First run - this account owns the instance.' : 'Welcome back.' }}
      </p>

      <form class="flex flex-col gap-3" @submit.prevent="submit">
        <label class="flex flex-col gap-1.5">
          <span class="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">email</span>
          <Input v-model="email" type="email" autocomplete="username" required placeholder="you@host" />
        </label>
        <label class="flex flex-col gap-1.5">
          <span class="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">password</span>
          <Input
            v-model="password"
            type="password"
            :autocomplete="isFirstRun ? 'new-password' : 'current-password'"
            required
            :placeholder="isFirstRun ? 'min 8 characters' : '••••••••'"
          />
        </label>
        <label v-if="isFirstRun" class="flex flex-col gap-1.5">
          <span class="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">confirm password</span>
          <Input
            v-model="confirm"
            type="password"
            autocomplete="new-password"
            required
            placeholder="repeat password"
          />
        </label>

        <p v-if="error" class="text-xs text-red-400">
          {{ error }}
        </p>

        <Button type="submit" :disabled="busy" class="mt-2 w-full">
          {{ busy ? 'Working...' : (isFirstRun ? 'Create account' : 'Sign in') }}
        </Button>
      </form>
    </div>
  </div>
</template>

<style scoped>
.logwall {
  mask-image: linear-gradient(to bottom, transparent, black 18%, black 82%, transparent);
}
.logwall-track {
  animation: logscroll 38s linear infinite;
}
@keyframes logscroll {
  from { transform: translateY(0); }
  to { transform: translateY(-50%); }
}
.glow {
  background:
    radial-gradient(40rem 40rem at 78% 18%, color-mix(in oklab, var(--color-sky-500) 14%, transparent), transparent 70%),
    radial-gradient(34rem 34rem at 20% 88%, color-mix(in oklab, var(--color-violet-500) 12%, transparent), transparent 70%);
}
.card {
  animation: reveal 0.4s cubic-bezier(0.22, 1, 0.36, 1) both;
}
@keyframes reveal {
  from { opacity: 0; transform: translateY(8px) scale(0.99); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}
@media (prefers-reduced-motion: reduce) {
  .logwall-track, .card { animation: none; }
}
</style>
