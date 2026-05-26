import { describe, expect, it } from 'vitest'
import { groupContainers, partitionPinned } from './group'

const c = (name: string, project = '') => ({ name, project })

describe('groupContainers', () => {
  it('groups items by project', () => {
    const groups = groupContainers([c('a', 'web'), c('b', 'api'), c('c', 'web')])
    expect(groups.map(g => g.project)).toEqual(['api', 'web'])
    expect(groups.find(g => g.project === 'web')!.items.map(i => i.name)).toEqual(['a', 'c'])
  })

  it('sorts items by name within a group', () => {
    const [group] = groupContainers([c('zeta', 'p'), c('alpha', 'p'), c('mid', 'p')])
    expect(group.items.map(i => i.name)).toEqual(['alpha', 'mid', 'zeta'])
  })

  it('orders named projects alphabetically and the unprojected bucket last', () => {
    const groups = groupContainers([c('x'), c('y', 'zoo'), c('z', 'apex')])
    expect(groups.map(g => g.project)).toEqual(['apex', 'zoo', ''])
  })

  it('returns no groups for an empty list', () => {
    expect(groupContainers([])).toEqual([])
  })
})

describe('partitionPinned', () => {
  it('splits pinned from the rest', () => {
    const { pinned, rest } = partitionPinned([c('a'), c('b'), c('d')], ['b'])
    expect(pinned.map(i => i.name)).toEqual(['b'])
    expect(rest.map(i => i.name)).toEqual(['a', 'd'])
  })

  it('orders pinned to match the pins list, not the input', () => {
    const { pinned } = partitionPinned([c('a'), c('b'), c('d')], ['d', 'a'])
    expect(pinned.map(i => i.name)).toEqual(['d', 'a'])
  })

  it('ignores pins with no matching container', () => {
    const { pinned, rest } = partitionPinned([c('a')], ['ghost'])
    expect(pinned).toEqual([])
    expect(rest.map(i => i.name)).toEqual(['a'])
  })
})
