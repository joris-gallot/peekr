import { describe, expect, it } from 'vitest'
import {
  type ParsedLog,
  formatTime,
  matchesFilter,
  parseFilter,
  parseLog,
  valueKind,
} from './logfmt'

describe('parseLog', () => {
  it('parses a pino-style JSON line', () => {
    const log = parseLog('{"level":"warn","msg":"slow query","ms":42}')
    expect(log.json).not.toBeNull()
    expect(log.level).toBe('warn')
    expect(log.message).toBe('slow query')
  })

  it('maps numeric pino levels', () => {
    expect(parseLog('{"level":50,"msg":"boom"}').level).toBe('error')
    expect(parseLog('{"level":10,"msg":"x"}').level).toBe('trace')
    expect(parseLog('{"level":30,"msg":"x"}').level).toBe('info')
  })

  it('normalizes string level aliases', () => {
    expect(parseLog('{"level":"ERR","msg":"x"}').level).toBe('error')
    expect(parseLog('{"level":"warning","msg":"x"}').level).toBe('warn')
    expect(parseLog('{"severity":"critical","msg":"x"}').level).toBe('fatal')
  })

  it('falls back to the raw line for the message when no msg key', () => {
    const log = parseLog('{"foo":"bar"}')
    expect(log.json).not.toBeNull()
    expect(log.level).toBeNull()
    expect(log.message).toBe('{"foo":"bar"}')
  })

  it('treats non-object JSON and plain text as unstructured', () => {
    expect(parseLog('[1,2,3]').json).toBeNull()
    const plain = parseLog('just a plain log line')
    expect(plain.json).toBeNull()
    expect(plain.level).toBeNull()
    expect(plain.message).toBe('just a plain log line')
  })

  it('does not throw on malformed JSON', () => {
    const log = parseLog('{"level":"info", oops}')
    expect(log.json).toBeNull()
    expect(log.message).toBe('{"level":"info", oops}')
  })
})

describe('parseFilter', () => {
  it('splits key=value terms and bare substrings', () => {
    expect(parseFilter('level=error user.id=123 boom')).toEqual([
      { path: 'level', value: 'error' },
      { path: 'user.id', value: '123' },
      { path: null, value: 'boom' },
    ])
  })

  it('returns no terms for empty input', () => {
    expect(parseFilter('   ')).toEqual([])
  })
})

describe('matchesFilter', () => {
  const log = parseLog(
    '{"level":"error","msg":"db timeout","user":{"id":123,"name":"ada"}}',
  )

  it('matches on normalized level', () => {
    expect(matchesFilter(log, parseFilter('level=error'))).toBe(true)
    expect(matchesFilter(log, parseFilter('level=info'))).toBe(false)
  })

  it('matches on a dotted field path', () => {
    expect(matchesFilter(log, parseFilter('user.id=123'))).toBe(true)
    expect(matchesFilter(log, parseFilter('user.name=ada'))).toBe(true)
    expect(matchesFilter(log, parseFilter('user.id=999'))).toBe(false)
  })

  it('matches a bare substring against the raw line', () => {
    expect(matchesFilter(log, parseFilter('timeout'))).toBe(true)
    expect(matchesFilter(log, parseFilter('missing'))).toBe(false)
  })

  it('requires every term (AND semantics)', () => {
    expect(matchesFilter(log, parseFilter('level=error timeout'))).toBe(true)
    expect(matchesFilter(log, parseFilter('level=error nope'))).toBe(false)
  })

  it('does not match field paths on plain logs', () => {
    const plain: ParsedLog = parseLog('plain line')
    expect(matchesFilter(plain, parseFilter('user.id=1'))).toBe(false)
  })
})

describe('valueKind', () => {
  it('classifies JSON value types', () => {
    expect(valueKind('s')).toBe('string')
    expect(valueKind(1)).toBe('number')
    expect(valueKind(true)).toBe('boolean')
    expect(valueKind(null)).toBe('null')
    expect(valueKind({ a: 1 })).toBe('complex')
    expect(valueKind([1])).toBe('complex')
  })
})

describe('formatTime', () => {
  it('renders HH:MM:SS from RFC3339', () => {
    expect(formatTime('2026-05-26T13:38:50.426Z')).toMatch(/^\d{2}:\d{2}:\d{2}$/)
  })

  it('returns empty for null or junk', () => {
    expect(formatTime(null)).toBe('')
    expect(formatTime('not-a-date')).toBe('')
  })
})
