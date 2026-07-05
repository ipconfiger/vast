import { describe, it, expect } from 'vitest'
import { getUserDisplayName } from './user'

describe('getUserDisplayName', () => {
  it('returns display_name when provided and non-empty', () => {
    expect(getUserDisplayName('Alice', 'alice1', 'uid-123')).toBe('Alice')
  })

  it('falls back to username when display_name is whitespace only', () => {
    expect(getUserDisplayName('   ', 'bob', 'uid-456')).toBe('bob')
  })

  it('falls back to id prefix when both display_name and username are empty/null', () => {
    expect(getUserDisplayName(null, '', 'uid-789')).toBe('uid-789')
  })

  it('returns "Unknown" when all arguments are empty/null', () => {
    expect(getUserDisplayName(undefined, undefined, undefined)).toBe('Unknown')
  })

  it('trims whitespace from display_name and username', () => {
    // display_name with whitespace should be trimmed
    expect(getUserDisplayName('  Carol  ', 'carol', 'uid')).toBe('Carol')
    // username with whitespace should be trimmed
    expect(getUserDisplayName('', '  dave  ', 'uid')).toBe('dave')
  })
})