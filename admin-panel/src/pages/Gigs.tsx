import { useEffect, useState } from 'react'
import { api, type AdminGigOut } from '../api'

export default function Gigs() {
  const [items, setItems] = useState<AdminGigOut[]>([])
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(1)
  const [q, setQ] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = async (p = page) => {
    setLoading(true); setError(null)
    try {
      const res = await api.listGigs({ page: p, per_page: 20, q: q || undefined })
      setItems(res.items); setTotal(res.total); setPage(res.page)
    } catch (e: any) { setError(e.message) }
    finally { setLoading(false) }
  }

  useEffect(() => { load(1) }, [])

  const remove = async (id: number) => {
    if (!confirm(`Delete gig #${id}?`)) return
    try { await api.deleteGig(id); await load(page) }
    catch (e: any) { setError(e.message) }
  }

  return (
    <div className="card">
      <h3 style={{ marginTop: 0 }}>Gigs moderation</h3>
      <div className="row" style={{ marginBottom: 10 }}>
        <input className="input" placeholder="Search title/description" value={q} onChange={e => setQ(e.target.value)} />
        <button className="btn" onClick={() => load(1)}>Search</button>
        <span className="muted">{total} total</span>
      </div>
      {error && <p className="error">{error}</p>}
      {loading ? <p className="muted">Loading…</p> : (
        <table>
          <thead><tr><th>ID</th><th>Title</th><th>Poster</th><th>Meta</th><th>Created</th><th></th></tr></thead>
          <tbody>
            {items.map(g => (
              <tr key={g.id}>
                <td>{g.id}</td>
                <td>
                  <b>{g.title}</b>
                  <div className="muted" style={{ maxWidth: 420, whiteSpace: 'pre-wrap' }}>{g.description}</div>
                </td>
                <td>@{g.poster_username} <span className="muted">(#{g.poster_id})</span></td>
                <td>
                  <span className="tag">{g.collar}</span> <span className="tag">{g.status}</span>
                  {g.location && <div className="muted">{g.location}</div>}
                </td>
                <td className="muted">{new Date(g.created_at).toLocaleString()}</td>
                <td><button className="btn danger" onClick={() => remove(g.id)}>Delete</button></td>
              </tr>
            ))}
            {items.length === 0 && <tr><td colSpan={6} className="muted">No gigs found.</td></tr>}
          </tbody>
        </table>
      )}
      <div className="row" style={{ marginTop: 10 }}>
        <button className="btn secondary" disabled={page <= 1} onClick={() => load(page - 1)}>Prev</button>
        <span className="muted">Page {page}</span>
        <button className="btn secondary" disabled={items.length < 20} onClick={() => load(page + 1)}>Next</button>
      </div>
    </div>
  )
}
