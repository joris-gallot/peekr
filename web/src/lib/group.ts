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
