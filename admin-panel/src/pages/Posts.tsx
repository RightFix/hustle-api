import { useEffect, useState } from 'react'
import { api, type AdminPostOut } from '../api'

export default function Posts() {
  const [items, setItems] = useState<AdminPostOut[]>([])
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(1)
  const [q, setQ] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = async (p = page) => {
    setLoading(true); setError(null)
    try {
      const res = await api.listPosts({ page: p, per_page: 20, q: q || undefined })
      setItems(res.items); setTotal(res.total); setPage(res.page)
    } catch (e: any) { setError(e.message) }
    finally { setLoading(false) }
  }

  useEffect(() => { load(1) }, [])

  const remove = async (id: number) => {
    if (!confirm(`Delete post #${id}?`)) return
    try { await api.deletePost(id); await load(page) }
    catch (e: any) { setError(e.message) }
  }

  return (
    <div className="card">
      <h3 style={{ marginTop: 0 }}>Posts moderation</h3>
      <div className="row" style={{ marginBottom: 10 }}>
        <input className="input" placeholder="Search content" value={q} onChange={e => setQ(e.target.value)} />
        <button className="btn" onClick={() => load(1)}>Search</button>
        <span className="muted">{total} total</span>
      </div>
      {error && <p className="error">{error}</p>}
      {loading ? <p className="muted">Loading…</p> : (
        <table>
          <thead><tr><th>ID</th><th>Author</th><th>Content</th><th>Created</th><th></th></tr></thead>
          <tbody>
            {items.map(p => (
              <tr key={p.id}>
                <td>{p.id}</td>
                <td>@{p.author_username} <span className="muted">(#{p.author_id})</span></td>
                <td style={{ maxWidth: 520, whiteSpace: 'pre-wrap' }}>{p.content}</td>
                <td className="muted">{new Date(p.created_at).toLocaleString()}</td>
                <td><button className="btn danger" onClick={() => remove(p.id)}>Delete</button></td>
              </tr>
            ))}
            {items.length === 0 && <tr><td colSpan={5} className="muted">No posts found.</td></tr>}
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
