import { useEffect, useState } from 'react'
import { api, type AdminUserDetail, type UserOut } from '../api'

export default function Users({ currentAdminId }: { currentAdminId: number }) {
  const [items, setItems] = useState<UserOut[]>([])
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(1)
  const [q, setQ] = useState('')
  const [status, setStatus] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [detail, setDetail] = useState<AdminUserDetail | null>(null)
  const [reason, setReason] = useState('')

  const load = async (p = page) => {
    setLoading(true); setError(null)
    try {
      const res = await api.listUsers({ page: p, per_page: 20, q: q || undefined, status: status || undefined })
      setItems(res.items); setTotal(res.total); setPage(res.page)
    } catch (e: any) { setError(e.message) }
    finally { setLoading(false) }
  }

  useEffect(() => { load(1) }, [status])

  const openDetail = async (id: number) => {
    try { setDetail(await api.getUser(id)); setReason('') }
    catch (e: any) { setError(e.message) }
  }

  const act = async (fn: () => Promise<any>, refreshId?: number) => {
    try {
      setError(null)
      await fn()
      await load(page)
      if (refreshId) setDetail(await api.getUser(refreshId))
    } catch (e: any) { setError(e.message) }
  }

  const u = detail?.user
  return (
    <div className="grid" style={{ gap: 14 }}>
      <div className="card">
        <h3 style={{ marginTop: 0 }}>Users</h3>
        <div className="row" style={{ marginBottom: 10 }}>
          <input className="input" placeholder="Search username/email" value={q} onChange={e => setQ(e.target.value)} />
          <select value={status} onChange={e => setStatus(e.target.value)}>
            <option value="">All</option>
            <option value="active">Active</option>
            <option value="suspended">Suspended</option>
          </select>
          <button className="btn" onClick={() => load(1)}>Search</button>
          <span className="muted">{total} total</span>
        </div>
        {error && <p className="error">{error}</p>}
        {loading ? <p className="muted">Loading…</p> : (
          <table>
            <thead><tr><th>ID</th><th>User</th><th>Email</th><th>Flags</th><th>Created</th><th></th></tr></thead>
            <tbody>
              {items.map(x => (
                <tr key={x.id}>
                  <td>{x.id}</td>
                  <td>@{x.username}{x.is_admin && <span className="tag" style={{ marginLeft: 6 }}>admin</span>}</td>
                  <td>{x.email}</td>
                  <td>
                    {x.is_suspended ? <span className="tag bad">suspended</span> : <span className="tag good">active</span>}{' '}
                    {x.email_verified ? <span className="tag good">verified</span> : <span className="tag">unverified</span>}
                  </td>
                  <td className="muted">{new Date(x.created_at).toLocaleString()}</td>
                  <td><button className="btn secondary" onClick={() => openDetail(x.id)}>Manage</button></td>
                </tr>
              ))}
              {items.length === 0 && <tr><td colSpan={6} className="muted">No users found.</td></tr>}
            </tbody>
          </table>
        )}
        <div className="row" style={{ marginTop: 10 }}>
          <button className="btn secondary" disabled={page <= 1} onClick={() => load(page - 1)}>Prev</button>
          <span className="muted">Page {page}</span>
          <button className="btn secondary" disabled={items.length < 20} onClick={() => load(page + 1)}>Next</button>
        </div>
      </div>

      {detail && u && (
        <div className="card">
          <div className="row" style={{ justifyContent: 'space-between' }}>
            <h3 style={{ margin: 0 }}>Manage @{u.username} (#{u.id})</h3>
            <button className="btn secondary" onClick={() => setDetail(null)}>Close</button>
          </div>
          <div className="grid stats" style={{ marginTop: 10 }}>
            <div><div className="muted">Posts</div><b>{detail.post_count}</b></div>
            <div><div className="muted">Gigs</div><b>{detail.gig_count}</b></div>
            <div><div className="muted">Applications</div><b>{detail.application_count}</b></div>
            <div><div className="muted">Conversations</div><b>{detail.conversation_count}</b></div>
          </div>
          <p className="muted">Email: {u.email} · Type: {u.account_type} · Verified: {String(u.email_verified)} · Admin: {String(u.is_admin)} · Suspended: {String(u.is_suspended)}</p>
          {u.suspended_reason && <p className="error">Reason: {u.suspended_reason}</p>}

          <div className="grid" style={{ gap: 10 }}>
            <div className="row">
              <input className="input" style={{ minWidth: 260 }} placeholder="Suspension reason" value={reason} onChange={e => setReason(e.target.value)} />
              {!u.is_suspended ? (
                <button className="btn danger" onClick={() => act(() => api.updateUser(u.id, { is_suspended: true, suspended_reason: reason || undefined }), u.id)}>Suspend</button>
              ) : (
                <button className="btn" onClick={() => act(() => api.updateUser(u.id, { is_suspended: false }), u.id)}>Unsuspend</button>
              )}
            </div>
            <div className="row">
              <button className="btn secondary" onClick={() => act(() => api.updateUser(u.id, { email_verified: !u.email_verified }), u.id)}>
                {u.email_verified ? 'Mark unverified' : 'Mark verified'}
              </button>
              <button className="btn secondary" disabled={u.id === currentAdminId} onClick={() => act(() => api.updateUser(u.id, { is_admin: !u.is_admin }), u.id)}>
                {u.is_admin ? 'Remove admin' : 'Make admin'}
              </button>
              <button className="btn danger" disabled={u.id === currentAdminId} onClick={() => { if (confirm(`Delete user @${u.username}? This cannot be undone.`)) act(async () => { await api.deleteUser(u.id); setDetail(null) }) }}>
                Delete user
              </button>
            </div>
            {u.id === currentAdminId && <p className="muted">You cannot demote, suspend, or delete your own account from here.</p>}
          </div>
        </div>
      )}
    </div>
  )
}
