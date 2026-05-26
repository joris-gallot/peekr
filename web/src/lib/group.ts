export interface Groupable {
  name: string
  project: string
}

export interface ContainerGroup<T> {
  project: string
  items: T[]
}

/** Group by compose project: named projects A-Z, the unprojected bucket ('') last. */
export function groupContainers<T extends Groupable>(list: T[]): ContainerGroup<T>[] {
  const buckets = new Map<string, T[]>()
  for (const item of list) {
    const key = item.project || ''
    const bucket = buckets.get(key)
    if (bucket)
      bucket.push(item)
    else buckets.set(key, [item])
  }

  const groups = [...buckets.entries()].map(([project, items]) => ({
    project,
    items: [...items].sort((a, b) => a.name.localeCompare(b.name)),
  }))

  groups.sort((a, b) => {
    if (a.project === '')
      return 1
    if (b.project === '')
      return -1
    return a.project.localeCompare(b.project)
  })
  return groups
}

/** Split out pinned items (by name), ordered to match the `pins` list. */
export function partitionPinned<T extends { name: string }>(
  list: T[],
  pins: string[],
): { pinned: T[], rest: T[] } {
  const pinSet = new Set(pins)
  const pinned: T[] = []
  const rest: T[] = []
  for (const item of list) {
    if (pinSet.has(item.name))
      pinned.push(item)
    else rest.push(item)
  }
  pinned.sort((a, b) => pins.indexOf(a.name) - pins.indexOf(b.name))
  return { pinned, rest }
}
