import { useAuthImage } from '../hooks/useAuthImage'

const SIZE_CLASSES: Record<string, { box: string; text: string }> = {
  xs: { box: 'h-6 w-6', text: 'text-[10px]' },
  sm: { box: 'h-8 w-8', text: 'text-xs' },
  md: { box: 'h-10 w-10', text: 'text-sm' },
  lg: { box: 'h-24 w-24', text: 'text-3xl' },
}

interface UserAvatarProps {
  avatarUrl?: string | null
  displayName: string
  size?: 'xs' | 'sm' | 'md' | 'lg'
  rounded?: 'full' | 'md'
  className?: string
}

export function UserAvatar({
  avatarUrl,
  displayName,
  size = 'sm',
  rounded = 'full',
  className = '',
}: UserAvatarProps) {
  const src = useAuthImage(avatarUrl)
  const { box, text } = SIZE_CLASSES[size]
  const roundedClass = rounded === 'md' ? 'rounded-md' : 'rounded-full'

  if (src) {
    return (
      <img
        src={src}
        alt={displayName}
        className={`${roundedClass} object-cover ${box} ${className}`}
      />
    )
  }

  return (
    <div
      className={`flex items-center justify-center ${roundedClass} bg-zinc-700 font-semibold text-zinc-300 ${box} ${text} ${className}`}
    >
      {displayName.charAt(0).toUpperCase() || '?'}
    </div>
  )
}
