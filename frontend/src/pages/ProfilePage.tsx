import { useState } from 'react'
import { useMutation } from '@tanstack/react-query'
import { apiClient } from '../api/client'
import { useAuthStore } from '../stores/authStore'
import { useNavigate } from 'react-router'
import { ArrowLeft, Save, CheckCircle } from 'lucide-react'

export default function ProfilePage() {
  const user = useAuthStore((s) => s.user)
  const setUser = useAuthStore((s) => s.setUser)
  const navigate = useNavigate()
  const [displayName, setDisplayName] = useState('')
  const [fetched, setFetched] = useState(false)
  const [saved, setSaved] = useState(false)

  const saveMutation = useMutation({
    mutationFn: (display_name: string) =>
      apiClient<{ id: string; username: string; display_name: string }>('/auth/profile', {
        method: 'PATCH',
        body: JSON.stringify({ display_name }),
      }),
    onSuccess: (data) => {
      if (user) setUser({ ...user, display_name: data.display_name })
      setSaved(true)
      setTimeout(() => setSaved(false), 3000)
    },
  })

  if (!fetched) {
    apiClient<{ id: string; username: string; display_name: string }>('/auth/profile')
      .then(d => { setDisplayName(d.display_name); setFetched(true) })
    return <div className="min-h-screen bg-slate-950 flex items-center justify-center"><div className="animate-spin h-6 w-6 border-2 border-indigo-500 border-t-transparent rounded-full" /></div>
  }

  return (
    <div className="min-h-screen bg-slate-950 px-4 py-8">
      <div className="max-w-md mx-auto">
        <button onClick={() => navigate(-1)} className="flex items-center gap-1 text-sm text-zinc-400 hover:text-zinc-200 mb-6">
          <ArrowLeft className="h-4 w-4" /> Back
        </button>
        <div className="bg-slate-900 rounded-2xl border border-slate-800 p-6">
          <h1 className="text-xl font-bold text-white mb-6">Profile</h1>
          <div className="space-y-4">
            <div>
              <label className="block text-sm text-zinc-400 mb-1">Username</label>
              <input readOnly value={user?.username || ''} className="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-zinc-400 text-sm" />
            </div>
            <div>
              <label className="block text-sm text-zinc-400 mb-1">Display Name</label>
              <input
                value={displayName}
                onChange={e => setDisplayName(e.target.value)}
                placeholder="Set a display name"
                maxLength={32}
                className="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
              />
              <p className="text-xs text-zinc-500 mt-1">Shown in chat instead of your username. 32 characters max.</p>
            </div>
            <button
              onClick={() => saveMutation.mutate(displayName)}
              disabled={saveMutation.isPending}
              className="w-full py-2 px-4 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded-lg flex items-center justify-center gap-2"
            >
              <Save className="h-4 w-4" />
              {saveMutation.isPending ? 'Saving...' : 'Save'}
            </button>
            {saved && (
              <div className="flex items-center gap-2 text-sm text-emerald-400 bg-emerald-500/10 rounded-lg px-3 py-2">
                <CheckCircle className="h-4 w-4" />
                Display name saved successfully!
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
