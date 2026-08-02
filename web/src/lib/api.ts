export async function api<T = unknown>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const res = await fetch(`/api${path}`, {
    headers: { "content-type": "application/json" },
    credentials: "same-origin",
    ...options,
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    const msg =
      (data as { error?: { message?: string } })?.error?.message ??
      `HTTP ${res.status}`;
    throw new Error(msg);
  }
  return data as T;
}

export interface AuthStatus {
  authenticated: boolean;
  require_login: boolean;
  password_set: boolean;
}

export interface ApiKey {
  id: string;
  key: string;
  name: string | null;
  is_active: boolean;
  token_limit: number | null;
  limit_window: string | null;
  rpm_limit: number | null;
  allowed_models: string[];
  limit_reset_at: string | null;
  created_at: string;
}
