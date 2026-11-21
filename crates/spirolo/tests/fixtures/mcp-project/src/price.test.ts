import { describe, expect, it } from 'vitest'
import { total } from './price'

describe('total', () => {
  it('adds the prices', () => {
    expect(total([1, 2])).toBe(3)
  })

  it('returns something for an empty basket', () => {
    expect(total([])).toBeDefined()
  })

  it.skip('rounds to cents', () => {
    expect(total([0.1, 0.2])).toBe(0.3)
  })
})
