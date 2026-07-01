import { Bell } from 'lucide-react'
import { usePendingRequestsCount } from '../api/permissions'
import { usePermissionStore } from '../stores/permissionStore'
import { useEffect } from 'react'

export function PendingRequestsBadge() {
  const { data: count, isLoading } = usePendingRequestsCount()
  const setPendingRequestsCount = usePermissionStore((s) => s.setPendingRequestsCount)

  useEffect(() => {
    if (count !== undefined) {
      setPendingRequestsCount(count)
    }
  }, [count, setPendingRequestsCount])

  if (isLoading || !count || count === 0) return null

  return (
    <div className="flex items-center gap-1.5 rounded-md bg-amber-500/10 border border-amber-500/20 px-2 py-1 text-xs text-amber-400">
      <Bell className="h-3 w-3" />
      <span>{count} pending</span>
    </div>
  )
}
