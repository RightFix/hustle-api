import { useEffect, useState } from 'react'
import { Link, NavLink, Route, Routes, Navigate } from 'react-router-dom'
import { api, type UserOut } from './api'
import Dashboard from './pages/Dashboard'
import Users from './pages/Users'
import Posts from './pages/Posts'
import Gigs from './pages/Gigs'

export default function App() {
  const [me, setMe] = useState<UserOut | null>(null)
  const [loading, setLoading] = useState(true)
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const refresh = async () => {
    try {
      const u = await api.me()
      if (!u.is_admin) {
        setError('This account is not an admin.')
        setMe(null)
      } else {
        setMe(u)
        setError(null)
      }
    } catch {
      setMe(null)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { refresh() }, [])

  const login = async (e: React.FormEvent) => {
    e.preventDefault()
    setBusy(true); setError(null)
    try {
      await api.login(email, password)
      await refresh()
    } catch (err: any) {
      setError(err.message || 'Login failed')
      setLoading(false)
    } finally { setBusy(false) }
  }

  const logout = async () => {
    try { await api.logout() } catch {}
    setMe(null)
  }

  if (loading) return <div className="container">Loading…</div>

  if (!me) {
    return (
      <div className="container" style={{ maxWidth: 420, marginTop: 60 }}>
        <div className="card">
          <h2 style={{ marginTop: 0 }}>Hustle Admin Login</h2>
          <p className="muted">Sign in with an admin account (cookie auth).</p>
          {error && <p className="error">{error}</p>}
          <form onSubmit={login} className="grid">
            <input className="input" placeholder="Email" value={email} onChange={e => setEmail(e.target.value)} />
            <input className="input" placeholder="Password" type="password" value={password} onChange={e => setPassword(e.target.value)} />
            <button className="btn" disabled={busy} type="submit">{busy ? 'Signing in…' : 'Sign in'}</button>
          </form>
        </div>
      </div>
    )
  }

  return (
    <div className="container">
      <div className="row" style={{ justifyContent: 'space-between', marginBottom: 8 }}>
        <h2 style={{ margin: 0 }}><Link to="/" style={{ color: 'inherit', textDecoration: 'none' }}>Hustle Admin</Link></h2>
        <div className="row">
          <span className="muted">@{me.username}</span>
          <button className="btn secondary" onClick={logout}>Logout</button>
        </div>
      </div>
      <nav className="nav">
        <NavLink to="/" end>Dashboard</NavLink>
        <NavLink to="/users">Users</NavLink>
        <NavLink to="/posts">Posts</NavLink>
        <NavLink to="/gigs">Gigs</NavLink>
      </nav>
      {error && <p className="error">{error}</p>}
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/users" element={<Users currentAdminId={me.id} />} />
        <Route path="/posts" element={<Posts />} />
        <Route path="/gigs" element={<Gigs />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </div>
  )
}
