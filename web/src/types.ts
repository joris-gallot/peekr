export interface ContainerInfo {
  id: string
  name: string
  image: string
  state: string
  status: string
  project: string
}

export interface HostInfo {
  id: string
  name: string
  status: string
}

export interface StatsSample {
  ts: number
  cpu_pct: number
  mem_used: number
  mem_limit: number
  mem_pct: number
}

export function isRunning(c: ContainerInfo): boolean {
  return c.state.toLowerCase().includes('running')
}
