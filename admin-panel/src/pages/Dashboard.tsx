import { useEffect, useState } from 'react'
import { api, type StatsOut } from '../api'

export default function Dashboard() {
  const [stats, setStats] = useState<StatsOut | null>(null)
  const [error, setError] = useState<string | null>(null)

  const load = async () => {
    try { setStats(await api.stats()); setError(null) }
    catch (e: any) { setError(e.message) }
  }
  useEffect(() => { load() }, [])

  return (
    <div className="grid" style={{ gap: 14 }}>
      <div className="row" style={{ justifyContent: 'space-between' }}>
        <h3 style={{ margin: 0 }}>Platform stats</h3>
        <button className="btn secondary" onClick={load}>Refresh</button>
      </div>
      {error && <p className="error">{error}</p>}
      {!stats ? <p className="muted">Loading…</p> : (
        <div className="grid stats">
          {[
            ['Users', stats.user_count],
            ['Posts', stats.post_count],
            ['Gigs', stats.gig_count],
            ['Conversations', stats.conversation_count],
            ['Messages', stats.message_count],
            ['Suspended users', stats.suspended_user_count],
            ['Unverified users', stats.unverified_user_count],
          ].map(([label, value]) => (
            <div key={String(label)} className="card">
              <div className="muted" style={{ fontSize: 13 }}>{label}</div>
              <div style={{ fontSize: 28, fontWeight: 700 }}>{value as number}</div>
            </div>
          ))}
        </div>
      )}
      <div className="card">
        <div className="muted">This dashboard uses: <code>/api/admin/stats</code>, <code>/api/admin/users</code> (list/get/patch/delete), <code>/api/admin/posts</code> (list/delete), <code>/api/admin/gigs</code> (list/delete), plus <code>/api/auth/login|logout</code> and <code>/api/users/me</code>.</div>
      </div>
    </div>
  )
}
