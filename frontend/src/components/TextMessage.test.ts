import { describe, it, expect } from 'vitest'
import { isRawUrl, codeExts } from './TextMessage'

describe('TextMessage - isRawUrl', () => {
  describe('should return false for non-raw URLs', () => {
    it('draw.io URLs should return false', () => {
      expect(isRawUrl('https://draw.io/diagram.png')).toBe(false)
    })

    it('strawberry.com URLs should return false even with .ts extension', () => {
      expect(isRawUrl('https://strawberry.com/pie.ts')).toBe(false)
    })

    it('arbitrary non-raw host URLs should return false', () => {
      expect(isRawUrl('https://example.com/file.rs')).toBe(false)
      expect(isRawUrl('https://myserver.com/code.py')).toBe(false)
    })
  })

  describe('should return true for allowed raw hosts', () => {
    it('raw.githubusercontent.com URLs should return true', () => {
      expect(isRawUrl('https://raw.githubusercontent.com/user/repo/main/file.rs')).toBe(true)
      expect(isRawUrl('https://raw.githubusercontent.com/x/y/main/f.rs')).toBe(true)
    })

    it('gist.githubusercontent.com URLs should return true', () => {
      expect(isRawUrl('https://gist.githubusercontent.com/user/gist/raw/file.py')).toBe(true)
    })

    it('gitlab.com with /raw/ path should return true', () => {
      expect(isRawUrl('https://gitlab.com/x/y/-/raw/main/f.go')).toBe(true)
      expect(isRawUrl('https://gitlab.com/group/project/-/raw/branch/file.ts')).toBe(true)
    })
  })

  describe('should handle edge cases', () => {
    it('invalid URLs should return false', () => {
      expect(isRawUrl('not-a-url')).toBe(false)
      expect(isRawUrl('')).toBe(false)
    })

    it('allowed raw host with non-code extension should return false', () => {
      expect(isRawUrl('https://raw.githubusercontent.com/user/repo/main/image.png')).toBe(false)
    })

    it('non-allowed host with code extension should return false', () => {
      expect(isRawUrl('https://example.com/file.ts')).toBe(false)
    })
  })
})

describe('TextMessage - codeExts', () => {
  it('should have no duplicate extensions', () => {
    const uniqueExtensions = new Set(codeExts)
    expect(uniqueExtensions.size).toBe(codeExts.length)
  })

  it('should contain expected extensions', () => {
    expect(codeExts).toContain('toml')
    expect(codeExts).toContain('jsx')
    expect(codeExts).toContain('tsx')
    expect(codeExts).toContain('svelte')
    expect(codeExts).toContain('rs')
    expect(codeExts).toContain('ts')
    expect(codeExts).toContain('py')
  })
})