import { useState, useEffect } from 'react'
import { useMutation } from '@tanstack/react-query'
import { apiClient } from '../api/client'
import { useUploadFile } from '../api/files'
import { useAuthStore } from '../stores/authStore'
import { toast } from '../stores/toastStore'
import { useNavigate } from 'react-router'
import { ArrowLeft, Save, CheckCircle, Camera, Bell, BellOff, Info } from 'lucide-react'

type NotifStatus = 'loading' | 'unsupported' | 'denied' | 'subscribed' | 'unsubscribed' | 'subscribing' | 'unsubscribing'

function urlBase64ToUint8Array(base64String: string): Uint8Array {
  const padding = '='.repeat((4 - base64String.length % 4) % 4)
  const base64 = (base64String + padding).replace(/-/g, '+').replace(/_/g, '/')
  const rawData = window.atob(base64)
  const buffer = new ArrayBuffer(rawData.length)
  const view = new Uint8Array(buffer)
  for (let i = 0; i < rawData.length; i++) {
    view[i] = rawData.charCodeAt(i)
  }
  return view
}

export default function ProfilePage() {
  const user = useAuthStore((s) => s.user)
  const setUser = useAuthStore((s) => s.setUser)
  const navigate = useNavigate()
  const [displayName, setDisplayName] = useState('')
  const [fetched, setFetched] = useState(false)
  const [saved, setSaved] = useState(false)
	const [uploading, setUploading] = useState(false)
	const [avatarSrc, setAvatarSrc] = useState<string | null>(null)
	  const uploadMutation = useUploadFile()
	const [notifStatus, setNotifStatus] = useState<NotifStatus>('loading')
	const [notifError, setNotifError] = useState<string | null>(null)
	const [dmPolicy, setDmPolicy] = useState<'open' | 'members'>('members')

  const saveMutation = useMutation({
    mutationFn: (params: { display_name: string; dm_policy: 'open' | 'members' }) =>
      apiClient<{ id: string; username: string; display_name: string; avatar_url: string; dm_policy: string }>('/auth/profile', {
        method: 'PATCH',
        body: JSON.stringify(params),
      }),
    onSuccess: (data) => {
      if (user) setUser({ ...user, display_name: data.display_name, avatar_url: data.avatar_url, dm_policy: data.dm_policy as 'open' | 'members' })
      setSaved(true)
      setTimeout(() => setSaved(false), 3000)
    },
  })

  useEffect(() => {
    if (!fetched) {
      apiClient<{ id: string; username: string; display_name: string; avatar_url: string; dm_policy?: string }>('/auth/profile')
        .then(d => {
          setDisplayName(d.display_name)
          setDmPolicy(d.dm_policy === 'open' ? 'open' : 'members')
          if (user) setUser({ ...user, avatar_url: d.avatar_url })
          setFetched(true)
        })
    }
  }, [])

  useEffect(() => {
    if (!user?.avatar_url) { setAvatarSrc(null); return }
    const token = useAuthStore.getState().token
    if (!token) return
    const controller = new AbortController()
    let objectUrl: string | null = null
    fetch(user.avatar_url, { signal: controller.signal, headers: { Authorization: `Bearer ${token}` } })
      .then(r => r.blob())
      .then(blob => {
        objectUrl = URL.createObjectURL(blob)
        setAvatarSrc(objectUrl)
      })
      .catch(() => {
        toast.error('Failed to load avatar')
      })
    return () => {
      if (objectUrl) URL.revokeObjectURL(objectUrl)
      controller.abort()
    }
	  }, [user?.avatar_url])

	  useEffect(() => {
	    if (!('serviceWorker' in navigator) || !('PushManager' in window)) {
	      setNotifStatus('unsupported')
	      return
	    }
	    // Show the button immediately — don't block the UI on getSubscription()
	    // which can hang indefinitely on Chrome (blocked FCM connection).
	    // Check subscription in background without awaiting.
	    setNotifStatus('unsubscribed')
	    if (navigator.serviceWorker.controller) {
	      navigator.serviceWorker.ready
	        .then(reg => reg.pushManager.getSubscription())
	        .then(sub => {
	          // Only update if user hasn't interacted yet
	          if (sub) setNotifStatus('subscribed')
	        })
	        .catch(() => {})
	    }
	  }, [])

	  async function handleEnableNotifications() {
	    setNotifStatus('subscribing')
	    setNotifError(null)

	    try {
	      if (!('serviceWorker' in navigator) || !('PushManager' in window)) {
	        setNotifStatus('unsupported')
	        return
	      }

	      if (!navigator.serviceWorker.controller) {
	        await navigator.serviceWorker.register('/sw.js')
	        await navigator.serviceWorker.ready
	      }

	      const permission = await Notification.requestPermission()
	      if (permission === 'denied') {
	        setNotifStatus('denied')
	        return
	      }
	      if (permission !== 'granted') {
	        setNotifStatus('unsubscribed')
	        return
	      }

	      const vapidData = await apiClient<{ public_key: string }>('/push/vapid-public-key')
	      const registration = await navigator.serviceWorker.ready
	      // pushManager.subscribe() can hang indefinitely if FCM is blocked
	      // (Chrome/Edge use Google FCM; Firefox uses Mozilla autopush).
	      const subscription = await Promise.race([
	        registration.pushManager.subscribe({
	          userVisibleOnly: true,
	          applicationServerKey: urlBase64ToUint8Array(vapidData.public_key) as unknown as BufferSource,
	        }),
	        new Promise<never>((_, reject) =>
	          setTimeout(() => reject(new Error('Timed out waiting for push service (Google FCM may be blocked). Try Firefox.')), 15000)
	        ),
	      ])

	      const subJson = subscription.toJSON()
	      await apiClient('/push/subscribe', {
	        method: 'POST',
	        body: JSON.stringify({
	          endpoint: subJson.endpoint,
	          p256dh: subJson.keys?.p256dh,
	          auth: subJson.keys?.auth,
	        }),
	      })

	      setNotifStatus('subscribed')
	      toast.success('Browser notifications enabled')
	    } catch (err) {
	      const message = err instanceof Error ? err.message : 'Failed to enable notifications'
	      setNotifError(message)
	      setNotifStatus('unsubscribed')
	      toast.error(message.includes('FCM') || message.includes('Timed out')
	        ? 'Push service unavailable. Try Firefox, or check your network.'
	        : 'Failed to enable notifications')
	    }
	  }

	  async function handleDisableNotifications() {
	    setNotifStatus('unsubscribing')
	    setNotifError(null)

	    try {
	      const registration = await navigator.serviceWorker.ready
	      const subscription = await registration.pushManager.getSubscription()

	      if (subscription) {
	        await subscription.unsubscribe()
	        await apiClient(`/push/unsubscribe?endpoint=${encodeURIComponent(subscription.endpoint)}`, {
	          method: 'DELETE',
	        })
	      }

	      setNotifStatus('unsubscribed')
	      toast.success('Browser notifications disabled')
	    } catch (err) {
	      const message = err instanceof Error ? err.message : 'Failed to disable notifications'
	      setNotifError(message)
	      setNotifStatus('subscribed')
	      toast.error('Failed to disable notifications')
	    }
	  }

  if (!fetched) {
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
            <div className="flex justify-center mb-6">
              <label className="relative cursor-pointer group">
                {avatarSrc ? (
                  <img src={avatarSrc} className="h-24 w-24 rounded-full object-cover border-2 border-zinc-700 group-hover:border-indigo-500 transition-colors" />
                ) : (
                  <div className="flex h-24 w-24 items-center justify-center rounded-full bg-zinc-700 text-3xl font-bold text-zinc-300 group-hover:ring-2 ring-indigo-500 transition-all">
                    {(user?.display_name || user?.username || '?').charAt(0).toUpperCase()}
                  </div>
                )}
                {uploading ? (
                  <div className="absolute inset-0 flex items-center justify-center rounded-full bg-black/70">
                    <div className="animate-spin h-5 w-5 border-2 border-white border-t-transparent rounded-full" />
                  </div>
                ) : (
                  <div className="absolute inset-0 flex items-center justify-center rounded-full bg-black/50 opacity-0 group-hover:opacity-100 transition-opacity">
                    <Camera className="h-6 w-6 text-white" />
                  </div>
                )}
                <input type="file" accept="image/*" className="hidden" onChange={async (e) => {
                  const file = e.target.files?.[0]
                  if (!file) return
                  setUploading(true)
                  // Phase flag prevents mislabeling a PATCH failure as "Upload
                  // failed" — that would trigger re-uploads and orphan files.
                  let phase: 'upload' | 'patch' = 'upload'
                  try {
                    const data = await uploadMutation.mutateAsync(file)
                    phase = 'patch'
                    await apiClient('/auth/profile', {
                      method: 'PATCH',
                      body: JSON.stringify({ avatar_url: data.url }),
                    })
                    if (user) setUser({ ...user, avatar_url: data.url })
                  } catch (err) {
                    if (phase === 'upload') {
                      const msg = err instanceof Error ? err.message : String(err)
                      toast.error('Upload failed: ' + msg)
                    } else {
                      toast.error('Image saved, profile update failed')
                    }
                  } finally {
                    setUploading(false)
                    e.target.value = ''
                  }
                }} />
              </label>
            </div>
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
            {/* DM policy toggle */}
            <div className="flex items-center justify-between rounded-lg bg-slate-800/50 border border-slate-700 px-4 py-3">
              <div className="flex-1 min-w-0">
                <label htmlFor="dm-policy-toggle" className="text-sm font-medium text-zinc-200 cursor-pointer">
                  Allow direct messages from anyone
                </label>
                <p className="text-xs text-zinc-500 mt-0.5">
                  When off, only people sharing a channel with you can send DMs
                </p>
              </div>
              <button
                id="dm-policy-toggle"
                type="button"
                role="switch"
                aria-checked={dmPolicy === 'open'}
                onClick={() => setDmPolicy(dmPolicy === 'open' ? 'members' : 'open')}
                className={`relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-indigo-500/50 ${
                  dmPolicy === 'open' ? 'bg-indigo-600' : 'bg-zinc-700'
                }`}
              >
                <span
                  className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
                    dmPolicy === 'open' ? 'translate-x-5' : 'translate-x-0'
                  }`}
                />
              </button>
            </div>
            <button
              onClick={() => saveMutation.mutate({ display_name: displayName, dm_policy: dmPolicy })}
              disabled={saveMutation.isPending}
              className="w-full py-2 px-4 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded-lg flex items-center justify-center gap-2"
            >
              <Save className="h-4 w-4" />
              {saveMutation.isPending ? 'Saving...' : 'Save'}
            </button>
            {saved && (
              <div className="flex items-center gap-2 text-sm text-emerald-400 bg-emerald-500/10 rounded-lg px-3 py-2">
                <CheckCircle className="h-4 w-4" />
                Profile saved successfully!
              </div>
            )}
          </div>
        </div>
        <div className="bg-slate-900 rounded-2xl border border-slate-800 p-6 mt-4">
          <h2 className="text-xl font-bold text-white mb-4">Notifications</h2>
          {notifStatus === 'loading' && (
            <div className="flex items-center gap-2 text-sm text-zinc-400">
              <div className="animate-spin h-4 w-4 border-2 border-indigo-500 border-t-transparent rounded-full" />
              Checking notification status...
            </div>
          )}
          {notifStatus === 'unsupported' && (
            <div className="flex items-start gap-2 text-sm text-zinc-400 bg-slate-800/50 rounded-lg px-3 py-3">
              <Info className="h-4 w-4 mt-0.5 shrink-0 text-zinc-500" />
              <span>Browser notifications are not supported in this browser.</span>
            </div>
          )}
          {notifStatus === 'denied' && (
            <div className="flex items-start gap-2 text-sm text-zinc-400 bg-slate-800/50 rounded-lg px-3 py-3">
              <Info className="h-4 w-4 mt-0.5 shrink-0 text-amber-400" />
              <span>Notification permission was denied. Enable notifications in your browser settings to receive push notifications.</span>
            </div>
          )}
          {(notifStatus === 'unsubscribed' || notifStatus === 'subscribing') && (
            <button
              onClick={handleEnableNotifications}
              disabled={notifStatus === 'subscribing'}
              className="w-full py-2 px-4 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded-lg flex items-center justify-center gap-2"
            >
              {notifStatus === 'subscribing' ? (
                <div className="animate-spin h-4 w-4 border-2 border-white border-t-transparent rounded-full" />
              ) : (
                <Bell className="h-4 w-4" />
              )}
              {notifStatus === 'subscribing' ? 'Enabling...' : 'Enable Browser Notifications'}
            </button>
          )}
          {(notifStatus === 'subscribed' || notifStatus === 'unsubscribing') && (
            <button
              onClick={handleDisableNotifications}
              disabled={notifStatus === 'unsubscribing'}
              className="w-full py-2 px-4 bg-red-600 hover:bg-red-500 disabled:opacity-50 text-white rounded-lg flex items-center justify-center gap-2"
            >
              {notifStatus === 'unsubscribing' ? (
                <div className="animate-spin h-4 w-4 border-2 border-white border-t-transparent rounded-full" />
              ) : (
                <BellOff className="h-4 w-4" />
              )}
              {notifStatus === 'unsubscribing' ? 'Disabling...' : 'Disable Notifications'}
            </button>
          )}
          {notifError && (
            <p className="text-xs text-red-400 mt-2">{notifError}</p>
          )}
        </div>
      </div>
    </div>
  )
}
