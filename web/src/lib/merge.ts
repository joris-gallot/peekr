export interface TimeOrdered {
  ts: string | null
  seq: number
}

/**
 * Merge already-ordered per-container lists into one timeline.
 * RFC3339 timestamps sort lexicographically; `seq` breaks ties (and orders
 * lines that carry no timestamp by arrival).
 */
export function mergeByTime<T extends TimeOrdered>(lists: T[][], limit = 2000): T[] {
  const all: T[] = []
  for (const list of lists) all.push(...list)
  all.sort((a, b) => {
    const ta = a.ts ?? ''
    const tb = b.ts ?? ''
    if (ta < tb)
      return -1
    if (ta > tb)
      return 1
    return a.seq - b.seq
  })
  return all.length > limit ? all.slice(all.length - limit) : all
}
