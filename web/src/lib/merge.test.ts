import { describe, expect, it } from 'vitest'
import { mergeByTime } from './merge'

const e = (ts: string | null, seq: number, label = '') => ({ ts, seq, label })

describe('mergeByTime', () => {
  it('interleaves lists by timestamp', () => {
    const a = [e('2026-01-01T00:00:01Z', 0, 'a1'), e('2026-01-01T00:00:03Z', 2, 'a2')]
    const b = [e('2026-01-01T00:00:02Z', 1, 'b1')]
    expect(mergeByTime([a, b]).map(x => x.label)).toEqual(['a1', 'b1', 'a2'])
  })

  it('breaks equal timestamps by seq (arrival order)', () => {
    const a = [e('2026-01-01T00:00:01Z', 5, 'a')]
    const b = [e('2026-01-01T00:00:01Z', 2, 'b')]
    expect(mergeByTime([a, b]).map(x => x.label)).toEqual(['b', 'a'])
  })

  it('orders null timestamps by seq, before dated lines', () => {
    const a = [e(null, 1, 'n1'), e(null, 3, 'n2')]
    const b = [e('2026-01-01T00:00:01Z', 2, 'd')]
    expect(mergeByTime([a, b]).map(x => x.label)).toEqual(['n1', 'n2', 'd'])
  })

  it('keeps only the last `limit` entries', () => {
    const list = Array.from({ length: 10 }, (_, i) => e(`2026-01-01T00:00:0${i}Z`, i, `l${i}`))
    const out = mergeByTime([list], 3)
    expect(out.map(x => x.label)).toEqual(['l7', 'l8', 'l9'])
  })
})
