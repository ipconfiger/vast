// Admin Console — layout shell with sidebar + topbar + <Outlet />.
// Sibling of the user AppLayout (inline in App.tsx), but does NOT
// mount useWebSocket / useKeyboardShortcuts — admin has no WS session.
import { NavLink, Outlet, useNavigate } from 'react-router'
import {
  Shield,
  LayoutDashboard,
  Users,
  Ticket,
  ScrollText,
  Bot,
  LogOut,
} from 'lucide-react'
import { useAdminAuthStore } from '../../stores/adminAuthStore'
import { adminLogout } from '../../api/admin'
import { ToastContainer } from '../ToastContainer'

const NAV_ITEMS = [
  { to: '/admin', label: 'Dashboard', icon: LayoutDashboard, end: true },
  { to: '/admin/users', label: 'Users', icon: Users, end: false },
  { to: '/admin/invite-codes', label: 'Invite Codes', icon: Ticket, end: false },
  { to: '/admin/bots', label: 'Bots', icon: Bot, end: false },
  { to: '/admin/audit-logs', label: 'Audit Logs', icon: ScrollText, end: false },
] as const

export default function AdminLayout() {
  const username = useAdminAuthStore((s) => s.username)
  const navigate = useNavigate()

  async function handleLogout() {
    // adminLogout calls backend /logout then clears local state in finally.
    // Swallow network errors — local state is cleared regardless.
    await adminLogout().catch(() => {})
    navigate('/admin/login')
  }

  return (
    <div className="flex h-screen bg-zinc-950 text-zinc-100">
      {/* Sidebar */}
      <aside className="flex w-60 flex-col border-r border-zinc-800 bg-zinc-900">
        <div className="flex items-center gap-2 border-b border-zinc-800 px-5 py-4">
          <Shield className="h-5 w-5 text-indigo-400" />
          <span className="font-semibold tracking-tight">Admin Console</span>
        </div>

        <nav className="flex-1 space-y-1 px-3 py-4">
          {NAV_ITEMS.map(({ to, label, icon: Icon, end }) => (
            <NavLink
              key={to}
              to={to}
              end={end}
              className={({ isActive }) =>
                `flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors ${
                  isActive
                    ? 'bg-zinc-800 text-white'
                    : 'text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200'
                }`
              }
            >
              <Icon className="h-4 w-4 flex-shrink-0" />
              {label}
            </NavLink>
          ))}
        </nav>

        <div className="border-t border-zinc-800 p-3">
          <button
            onClick={handleLogout}
            className="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm text-zinc-400 transition-colors hover:bg-zinc-800 hover:text-zinc-200"
          >
            <LogOut className="h-4 w-4 flex-shrink-0" />
            Logout
          </button>
        </div>
      </aside>

      {/* Main area */}
      <div className="flex flex-1 flex-col overflow-hidden">
        {/* Topbar */}
        <header className="flex items-center justify-between border-b border-zinc-800 px-6 py-3">
          <span className="inline-flex items-center rounded border border-indigo-500/20 bg-indigo-500/10 px-2 py-0.5 text-xs font-medium text-indigo-400">
            Admin
          </span>
          <span className="text-sm text-zinc-400">{username ?? 'admin'}</span>
        </header>

        {/* Page content */}
        <main className="flex-1 overflow-auto p-6">
          <Outlet />
        </main>
      </div>

      {/* Toast notifications */}
      <ToastContainer />
    </div>
  )
}
