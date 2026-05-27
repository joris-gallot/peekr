import type { Ref } from 'vue'
import type { ParsedLog } from '@/lib/logfmt'
import { ref } from 'vue'
import { parseLog } from '@/lib/logfmt'

export type ConnState = 'connecting' | 'open' | 'error'

export interface Entry {
  cid: string
  ts: string | null
  stream: 'stdout' | 'stderr'
  log: ParsedLog
  seq: number
}

interface LogLine {
  ts: string | null
  stream: 'stdout' | 'stderr'
  msg: string
}

export interface LogStream {
  id: string
  entries: Ref<Entry[]>
  conn: Ref<ConnState>
  close: () => void
}

// global so interleaved streams keep a stable arrival order across containers
let SEQ = 0

export function useLogStream(host: string, id: string, cap = 2000): LogStream {
  const entries = ref<Entry[]>([])
  const conn = ref<ConnState>('connecting')
  let source: EventSource | null = null
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  let lastTs: string | null = null

  function open() {
    source?.close()
    conn.value = 'connecting'
    // `since` resumes after a drop instead of re-dumping the tail (avoids dup lines)
    const since = lastTs ? Math.floor(Date.parse(lastTs) / 1000) : null
    const base = `/api/hosts/${host}/containers/${id}/logs`
    const url = since ? `${base}?since=${since}` : base

    source = new EventSource(url)
    source.onopen = () => (conn.value = 'open')
    source.onmessage = e => append(JSON.parse(e.data) as LogLine)
    source.addEventListener('stream-error', () => (conn.value = 'error'))
    source.onerror = () => {
      conn.value = 'error'
      // stop EventSource native retry (it drops `since`); resume ourselves
      source?.close()
      reconnectTimer = setTimeout(open, 2000)
    }
  }

  function append(line: LogLine) {
    // monotonic dedup: timestamps are ordered, so skip anything <= last seen
    if (line.ts && lastTs && line.ts <= lastTs)
      return
    if (line.ts)
      lastTs = line.ts
    entries.value.push({ cid: id, ts: line.ts, stream: line.stream, log: parseLog(line.msg), seq: SEQ++ })
    if (entries.value.length > cap)
      entries.value.splice(0, entries.value.length - cap)
  }

  function close() {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer)
      reconnectTimer = null
    }
    source?.close()
    source = null
  }

  open()
  return { id, entries, conn, close }
}
