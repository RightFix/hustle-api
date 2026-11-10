import { useEffect, useState } from 'react'

type AdminReport = {
  id: number
  reporter_id: number
  reporter_username: string
  target_type: string
  target_id: number
  target_label: string | null
  reason: string
  details: string | null
  status: string
  admin_note: string | null
  resolved_by: number | null
  resolved_by_username: string | null
  resolved_at: string | null
  created_at: string
}

type Page = { items: AdminReport[]; total: number; page: number; per_page: number }

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, { credentials: 'include', headers: { 'Content-Type': 'application/json', ...(init?.headers || {}) }, ...init })
  if (!res.ok) {
    let msg = `HTTP ${res.status}`
    try { msg = (await res.json()).detail || msg } catch {}
    throw new Error(msg)
  }
  return res.json() as Promise<T>
}

export default function Reports() {
  const [data, setData] = useState<Page | null>(null)
  const [status, setStatus] = useState('open')
  const [targetType, setTargetType] = useState('')
  const [page, setPage] = useState(1)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notes, setNotes] = useState<Record<number, string>>({})

  const load = async (p = page, s = status, t = targetType) => {
    setLoading(true); setError(null)
    try {
      const qs = new URLSearchParams({ page: String(p), per_page: '20', status: s })
      if (t) qs.set('target_type', t)
      setData(await req<Page>(`/api/admin/reports?${qs.toString()}`))
      setPage(p)
    } catch (e: any) { setError(e.message) }
    finally { setLoading(false) }
  }

  useEffect(() => { load(1, status, targetType) }, [status, targetType])

  const resolve = async (id: number, next: 'resolved' | 'dismissed') => {
    try {
      setError(null)
      await req(`/api/admin/reports/${id}`, { method: 'PATCH', body: JSON.stringify({ status: next, admin_note: notes[id] || undefined }) })
      await load(page)
    } catch (e: any) { setError(e.message) }
  }

  return (
    <div className="card">
      <h3 style={{ marginTop: 0 }}>Reports queue</h3>
      <div className="row" style={{ marginBottom: 10 }}>
        <select value={status} onChange={e => setStatus(e.target.value)}>
          <option value="open">Open</option>
          <option value="resolved">Resolved</option>
          <option value="dismissed">Dismissed</option>
          <option value="all">All</option>
        </select>
        <select value={targetType} onChange={e => setTargetType(e.target.value)}>
          <option value="">All targets</option>
          <option value="user">Users</option>
          <option value="post">Posts</option>
          <option value="gig">Gigs</option>
        </select>
        <button className="btn secondary" onClick={() => load(1)}>Refresh</button>
        <span className="muted">{data?.total ?? 0} total</span>
      </div>
      {error && <p className="error">{error}</p>}
      {loading ? <p className="muted">Loading…</p> : (
        <table>
          <thead><tr><th>ID</th><th>Reporter</th><th>Target</th><th>Reason</th><th>Status</th><th>Action</th></tr></thead>
          <tbody>
            {(data?.items || []).map(r => (
              <tr key={r.id}>
                <td>{r.id}</td>
                <td>@{r.reporter_username} <span className="muted">(#{r.reporter_id})</span></td>
                <td>
                  <span className="tag">{r.target_type}</span> #{r.target_id}
                  {r.target_label && <div className="muted" style={{ maxWidth: 260 }}>{r.target_label}</div>}
                </td>
                <td style={{ maxWidth: 320 }}>
                  <b>{r.reason}</b>
                  {r.details && <div className="muted" style={{ whiteSpace: 'pre-wrap' }}>{r.details}</div>}
                  <div className="muted" style={{ fontSize: 12 }}>{new Date(r.created_at).toLocaleString()}</div>
                </td>
                <td>
                  <span className={`tag ${r.status === 'open' ? 'bad' : 'good'}`}>{r.status}</span>
                  {r.admin_note && <div className="muted">Note: {r.admin_note}</div>}
                  {r.resolved_by_username && <div className="muted">by @{r.resolved_by_username}</div>}
                </td>
                <td style={{ minWidth: 240 }}>
                  {r.status === 'open' ? (
                    <div className="grid" style={{ gap: 6 }}>
                      <input className="input" placeholder="Admin note (optional)" value={notes[r.id] || ''} onChange={e => setNotes(n => ({ ...n, [r.id]: e.target.value }))} />
                      <div className="row">
                        <button className="btn" onClick={() => resolve(r.id, 'resolved')}>Resolve</button>
                        <button className="btn secondary" onClick={() => resolve(r.id, 'dismissed')}>Dismiss</button>
                      </div>
                    </div>
                  ) : <span className="muted">—</span>}
                </td>
              </tr>
            ))}
            {(data?.items?.length ?? 0) === 0 && <tr><td colSpan={6} className="muted">No reports.</td></tr>}
          </tbody>
        </table>
      )}
      <div className="row" style={{ marginTop: 10 }}>
        <button className="btn secondary" disabled={page <= 1} onClick={() => load(page - 1)}>Prev</button>
        <span className="muted">Page {page}</span>
        <button className="btn secondary" disabled={(data?.items.length ?? 0) < 20} onClick={() => load(page + 1)}>Next</button>
      </div>
    </div>
  )
}
