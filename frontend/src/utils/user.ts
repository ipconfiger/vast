/**
 * Unified display name logic across the entire app.
 *
 * Rule: display_name (nickname) → username → id prefix → 'Unknown'
 */

/**
 * Canonical user display-name resolver. All UI must route through this (or through userStore.getName which delegates here).
 * Fallback chain: display_name → username → id.slice(0,8) → 'Unknown'.
 */
export function getUserDisplayName(
  displayName?: string | null,
  username?: string | null,
  id?: string | null,
): string {
  const trimmedDisplayName = displayName?.trim()
  if (trimmedDisplayName) return trimmedDisplayName

  const trimmedUsername = username?.trim()
  if (trimmedUsername) return trimmedUsername

  if (id) return id.slice(0, 8)
  return 'Unknown'
}
