export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'fatal'

export interface ParsedLog {
  raw: string
  json: Record<string, unknown> | null
  level: LogLevel | null
  message: string
  expanded: boolean
}

const LEVEL_KEYS = ['level', 'lvl', 'severity', 'levelname', 'loglevel']
const MSG_KEYS = ['msg', 'message', 'text', 'log']

const PINO_LEVELS: Record<number, LogLevel> = {
  10: 'trace',
  20: 'debug',
  30: 'info',
  40: 'warn',
  50: 'error',
  60: 'fatal',
}

const STRING_ALIASES: Record<string, LogLevel> = {
  trace: 'trace',
  debug: 'debug',
  info: 'info',
  information: 'info',
  notice: 'info',
  warn: 'warn',
  warning: 'warn',
  error: 'error',
  err: 'error',
  fatal: 'fatal',
  crit: 'fatal',
  critical: 'fatal',
  panic: 'fatal',
}

function normalizeLevel(v: unknown): LogLevel | null {
  if (typeof v === 'number')
    return PINO_LEVELS[v] ?? null
  if (typeof v === 'string') {
    const s = v.trim().toLowerCase()
    if (s in STRING_ALIASES)
      return STRING_ALIASES[s]
    const n = Number(s)
    if (!Number.isNaN(n) && n in PINO_LEVELS)
      return PINO_LEVELS[n]
  }
  return null
}

export function parseLog(raw: string): ParsedLog {
  const trimmed = raw.trimStart()
  if (trimmed.startsWith('{') && trimmed.endsWith('}')) {
    try {
      const value: unknown = JSON.parse(trimmed)
      if (value && typeof value === 'object' && !Array.isArray(value)) {
        const json = value as Record<string, unknown>
        let level: LogLevel | null = null
        for (const k of LEVEL_KEYS) {
          if (k in json) {
            level = normalizeLevel(json[k])
            if (level)
              break
          }
        }
        let message = ''
        for (const k of MSG_KEYS) {
          if (typeof json[k] === 'string') {
            message = json[k] as string
            break
          }
        }
        return { raw, json, level, message: message || trimmed, expanded: false }
      }
    }
    catch {
      // not JSON, fall through to plain
    }
  }
  return { raw, json: null, level: null, message: raw, expanded: false }
}

export interface FilterTerm {
  path: string | null
  value: string
}

export function parseFilter(query: string): FilterTerm[] {
  return query
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map((token) => {
      const eq = token.indexOf('=')
      if (eq > 0)
        return { path: token.slice(0, eq), value: token.slice(eq + 1).toLowerCase() }
      return { path: null, value: token.toLowerCase() }
    })
}

function getPath(obj: Record<string, unknown>, path: string): unknown {
  return path.split('.').reduce<unknown>((acc, key) => {
    if (acc && typeof acc === 'object')
      return (acc as Record<string, unknown>)[key]
    return undefined
  }, obj)
}

export function matchesFilter(log: ParsedLog, terms: FilterTerm[]): boolean {
  return terms.every((term) => {
    if (term.path) {
      if (term.path === 'level')
        return (log.level ?? '').includes(term.value)
      if (!log.json)
        return false
      const v = getPath(log.json, term.path)
      if (v == null)
        return false
      return String(v).toLowerCase().includes(term.value)
    }
    return log.raw.toLowerCase().includes(term.value)
  })
}

/** RFC3339 -> HH:MM:SS, empty on parse failure. */
export function formatTime(ts: string | null): string {
  if (!ts)
    return ''
  const d = new Date(ts)
  if (Number.isNaN(d.getTime()))
    return ''
  return d.toTimeString().slice(0, 8)
}

export type JsonValueKind = 'string' | 'number' | 'boolean' | 'null' | 'complex'

export function valueKind(v: unknown): JsonValueKind {
  if (v === null)
    return 'null'
  if (typeof v === 'string')
    return 'string'
  if (typeof v === 'number')
    return 'number'
  if (typeof v === 'boolean')
    return 'boolean'
  return 'complex'
}

export function displayValue(v: unknown): string {
  if (v === null)
    return 'null'
  if (typeof v === 'object')
    return JSON.stringify(v)
  return String(v)
}
