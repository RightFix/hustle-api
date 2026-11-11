import { useEffect, useState } from 'react'

type AuditItem = {
  id: number
  actor_id: number | null
  actor_username: string | null
  action: string
  target_type: string
  target_id: number | null
  metadata: any
  created_at: string
}

type Page = { items: AuditItem[]; total: number; page: number; per_page: number }

async function req<T>(path: string): Promise<T> {
  const res = await fetch(path, { credentials: 'include' })
  if (!res.ok) {
    let msg = `HTTP ${res.status}`
    try { msg = (await res.json()).detail || msg } catch {}
    throw new Error(msg)
  }
  return res.json() as Promise<T>
}

export default function Audit() {
  const [data, setData] = useState<Page | null>(null)
  const [page, setPage] = useState(1)
  const [action, setAction] = useState('')
  const [targetType, setTargetType] = useState('')
  const [actorId, setActorId] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = async (p = page) => {
    setLoading(true); setError(null)
    try {
      const qs = new URLSearchParams({ page: String(p), per_page: '30' })
      if (action.trim()) qs.set('action', action.trim())
      if (targetType.trim()) qs.set('target_type', targetType.trim())
      if (actorId.trim()) qs.set('actor_id', actorId.trim())
      setData(await req<Page>(`/api/admin/audit?${qs.toString()}`))
      setPage(p)
    } catch (e: any) { setError(e.message) }
    finally { setLoading(false) }
  }

  useEffect(() => { load(1) }, [])

  return (
    <div className="card">
      <div className="row" style={{ justifyContent: 'space-between', marginBottom: 10, flexWrap: 'wrap' }}>
        <h3 style={{ margin: 0 }}>Admin audit log</h3>
        <div className="row" style={{ flexWrap: 'wrap' }}>
          <input className="input" placeholder="Action (e.g. user.suspend)" value={action} onChange={e => setAction(e.target.value)} style={{ width: 180 }} />
          <input className="input" placeholder="Target type" value={targetType} onChange={e => setTargetType(e.target.value)} style={{ width: 120 }} />
          <input className="input" placeholder="Actor ID" value={actorId} onChange={e => setActorId(e.target.value)} style={{ width: 100 }} />
          <button className="btn secondary" onClick={() => load(1)}>Apply</button>
          <button className="btn secondary" onClick={() => load(page)}>Refresh</button>
        </div>
      </div>
      <p className="muted" style={{ marginTop: 0 }}>{data?.total ?? 0} total actions</p>
      {error && <p className="error">{error}</p>}
      {loading ? <p className="muted">Loading…</p> : (
        <table>
          <thead><tr><th>When</th><th>Actor</th><th>Action</th><th>Target</th><th>Metadata</th></tr></thead>
          <tbody>
            {(data?.items || []).map(a => (
              <tr key={a.id}>
                <td className="muted" style={{ whiteSpace: 'nowrap' }}>{new Date(a.created_at).toLocaleString()}</td>
                <td>{a.actor_username ? `@${a.actor_username}` : '—'} {a.actor_id != null ? <span className="muted">(#{a.actor_id})</span> : null}</td>
                <td><span className="tag">{a.action}</span></td>
                <td>{a.target_type}{a.target_id != null ? ` #${a.target_id}` : ''}</td>
                <td className="muted" style={{ maxWidth: 360, whiteSpace: 'pre-wrap', fontSize: 12 }}>{JSON.stringify(a.metadata)}</td>
              </tr>
            ))}
            {(data?.items?.length ?? 0) === 0 && <tr><td colSpan={5} className="muted">No audit entries yet.</td></tr>}
          </tbody>
        </table>
      )}
      <div className="row" style={{ marginTop: 10 }}>
        <button className="btn secondary" disabled={page <= 1} onClick={() => load(page - 1)}>Prev</button>
        <span className="muted">Page {page}</span>
        <button className="btn secondary" disabled={(data?.items.length ?? 0) < 30} onClick={() => load(page + 1)}>Next</button>
      </div>
    </div>
  )
}
