export type UserOut = {
  id: number
  email: string
  username: string
  bio: string | null
  is_admin: boolean
  account_type: string
  location: string | null
  availability: string | null
  expected_rate: string | null
  languages: string | null
  avatar_url: string | null
  email_verified: boolean
  email_messages_enabled: boolean
  is_suspended: boolean
  suspended_reason: string | null
  suspended_at: string | null
  created_at: string
}

export type StatsOut = {
  user_count: number
  post_count: number
  gig_count: number
  conversation_count: number
  message_count: number
  suspended_user_count: number
  unverified_user_count: number
}

export type PaginatedUsers = { items: UserOut[]; total: number; page: number; per_page: number }
export type AdminUserDetail = { user: UserOut; post_count: number; gig_count: number; application_count: number; conversation_count: number }
export type AdminPostOut = { id: number; author_id: number; author_username: string; content: string; created_at: string }
export type PaginatedAdminPosts = { items: AdminPostOut[]; total: number; page: number; per_page: number }
export type AdminGigOut = { id: number; poster_id: number; poster_username: string; title: string; description: string; collar: string; status: string; location: string | null; created_at: string }
export type PaginatedAdminGigs = { items: AdminGigOut[]; total: number; page: number; per_page: number }
export type Message = { detail: string }

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    credentials: 'include',
    headers: { 'Content-Type': 'application/json', ...(init?.headers || {}) },
    ...init,
  })
  if (!res.ok) {
    let msg = `HTTP ${res.status}`
    try {
      const data = await res.json()
      msg = data.detail || msg
    } catch {}
    throw new Error(msg)
  }
  if (res.status === 204) return undefined as T
  return res.json() as Promise<T>
}

const qs = (params: Record<string, string | number | undefined>) => {
  const sp = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== '') sp.set(k, String(v))
  }
  const s = sp.toString()
  return s ? `?${s}` : ''
}

export const api = {
  login: (email: string, password: string) =>
    request<{ access_token: string; token_type: string }>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify({ email, password }),
    }),
  logout: () => request<Message>('/api/auth/logout', { method: 'POST' }),
  me: () => request<UserOut>('/api/users/me'),

  stats: () => request<StatsOut>('/api/admin/stats'),

  listUsers: (p: { page?: number; per_page?: number; q?: string; status?: string }) =>
    request<PaginatedUsers>(`/api/admin/users${qs(p)}`),
  getUser: (id: number) => request<AdminUserDetail>(`/api/admin/users/${id}`),
  updateUser: (
    id: number,
    data: { is_suspended?: boolean; suspended_reason?: string; email_verified?: boolean; is_admin?: boolean },
  ) => request<UserOut>(`/api/admin/users/${id}`, { method: 'PATCH', body: JSON.stringify(data) }),
  deleteUser: (id: number) => request<Message>(`/api/admin/users/${id}`, { method: 'DELETE' }),

  listPosts: (p: { page?: number; per_page?: number; q?: string }) =>
    request<PaginatedAdminPosts>(`/api/admin/posts${qs(p)}`),
  deletePost: (id: number) => request<Message>(`/api/admin/posts/${id}`, { method: 'DELETE' }),

  listGigs: (p: { page?: number; per_page?: number; q?: string }) =>
    request<PaginatedAdminGigs>(`/api/admin/gigs${qs(p)}`),
  deleteGig: (id: number) => request<Message>(`/api/admin/gigs/${id}`, { method: 'DELETE' }),
}
