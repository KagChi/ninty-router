import { Show, createResource, createSignal } from "solid-js";
import { api } from "~/lib/api";

interface Settings {
  rtk_enabled: boolean;
  caveman_enabled: boolean;
  ponytail_enabled: boolean;
  ponytail_level: string;
  pxpipe_enabled: boolean;
  pxpipe_min_chars: number;
  pxpipe_timeout_ms: number;
  require_api_key: boolean;
  require_login: boolean;
  enable_request_logs: boolean;
  sticky_round_robin_limit: number;
  [k: string]: unknown;
}

interface PxpipeStatus {
  installed: boolean;
  installing: boolean;
  version: string | null;
  npmAvailable: boolean;
}

export default function SettingsPage() {
  const [settings, { refetch }] = createResource(async () => api<Settings>("/settings"));
  const [saving, setSaving] = createSignal(false);
  const [msg, setMsg] = createSignal("");

  const patch = async (body: Record<string, unknown>) => {
    setSaving(true);
    setMsg("");
    try {
      await api("/settings", { method: "PATCH", body: JSON.stringify(body) });
      await refetch();
      setMsg("saved");
    } catch (e) {
      setMsg(e instanceof Error ? e.message : "failed");
    } finally {
      setSaving(false);
    }
  };

  const toggle = (key: keyof Settings) => {
    const cur = settings();
    if (!cur) return;
    patch({ [key]: !cur[key] });
  };

  const [pxpipe, { refetch: refetchPxpipe }] = createResource(async () =>
    api<PxpipeStatus>("/pxpipe/status")
  );
  const installPxpipe = async () => {
    await api("/pxpipe/install", { method: "POST" });
    // install runs server-side in background; poll status
    const t = setInterval(async () => {
      await refetchPxpipe();
      const st = pxpipe();
      if (st && !st.installing) clearInterval(t);
    }, 3000);
    setTimeout(() => clearInterval(t), 5 * 60 * 1000 + 5000);
  };

  return (
    <div>
      <h1 class="mb-4 text-xl font-semibold">Settings</h1>
      <Show when={settings()} fallback={<p class="text-sm text-text-muted">Loading…</p>}>
        {(s) => (
          <div class="max-w-xl space-y-6">
            <section class="rounded-lg border border-border bg-surface p-4">
              <h2 class="mb-3 font-medium">Token Savers</h2>

              <label class="mb-3 flex items-center justify-between text-sm">
                <span>
                  RTK compression
                  <span class="block text-xs text-text-muted">
                    Compress tool outputs (git diff, grep, ls, build logs…) before sending upstream
                  </span>
                </span>
                <input
                  type="checkbox"
                  checked={s().rtk_enabled}
                  onChange={() => toggle("rtk_enabled")}
                />
              </label>

              <label class="mb-3 flex items-center justify-between text-sm">
                <span>
                  Caveman
                  <span class="block text-xs text-text-muted">
                    Inject terse-response system prompt
                  </span>
                </span>
                <input
                  type="checkbox"
                  checked={s().caveman_enabled}
                  onChange={() => toggle("caveman_enabled")}
                />
              </label>

              <label class="mb-3 flex items-center justify-between text-sm">
                <span>
                  Ponytail
                  <span class="block text-xs text-text-muted">
                    Inject lazy-senior-dev system prompt (minimal code bias)
                  </span>
                </span>
                <input
                  type="checkbox"
                  checked={s().ponytail_enabled}
                  onChange={() => toggle("ponytail_enabled")}
                />
              </label>

              <label class="flex items-center justify-between text-sm">
                <span>Ponytail level</span>
                <select
                  class="rounded border border-border bg-bg px-2 py-1 text-sm"
                  value={s().ponytail_level}
                  onChange={(e) => patch({ ponytail_level: e.currentTarget.value })}
                >
                  <option value="lite">lite</option>
                  <option value="full">full</option>
                  <option value="ultra">ultra</option>
                </select>
              </label>

              <hr class="my-3 border-border" />

              <label class="mb-3 flex items-center justify-between text-sm">
                <span>
                  PXPIPE
                  <span class="block text-xs text-text-muted">
                    Render bulky Claude-format context as dense images (cheaper tokens). Requires
                    pxpipe-proxy install + node/bun.
                  </span>
                </span>
                <input
                  type="checkbox"
                  checked={s().pxpipe_enabled}
                  onChange={() => toggle("pxpipe_enabled")}
                />
              </label>

              <Show when={s().pxpipe_enabled}>
                <label class="mb-3 flex items-center justify-between text-sm">
                  <span>Min chars (gate)</span>
                  <input
                    type="number"
                    min="1000"
                    step="1000"
                    class="w-28 rounded border border-border bg-bg px-2 py-1 text-sm"
                    value={s().pxpipe_min_chars}
                    onChange={(e) =>
                      patch({ pxpipe_min_chars: Number(e.currentTarget.value) || 25000 })
                    }
                  />
                </label>
                <label class="mb-3 flex items-center justify-between text-sm">
                  <span>Timeout (ms)</span>
                  <input
                    type="number"
                    min="1000"
                    step="1000"
                    class="w-28 rounded border border-border bg-bg px-2 py-1 text-sm"
                    value={s().pxpipe_timeout_ms}
                    onChange={(e) =>
                      patch({ pxpipe_timeout_ms: Number(e.currentTarget.value) || 15000 })
                    }
                  />
                </label>
                <div class="flex items-center justify-between text-sm">
                  <span class="text-xs text-text-muted">
                    {pxpipe()?.installed
                      ? `pxpipe-proxy v${pxpipe()?.version ?? "?"} installed`
                      : pxpipe()?.installing
                        ? "installing…"
                        : pxpipe()?.npmAvailable
                          ? "not installed"
                          : "no JS runtime (node/bun) found"}
                  </span>
                  <Show when={!pxpipe()?.installed && !pxpipe()?.installing}>
                    <button
                      class="rounded border border-border px-2 py-1 text-xs hover:bg-bg"
                      onClick={installPxpipe}
                    >
                      Install pxpipe-proxy
                    </button>
                  </Show>
                </div>
              </Show>

              <p class="mt-3 text-xs text-text-muted">
                Per-request bypass: header <code>x-9router-token-saver: off</code>. Savings are
                recorded per request (usage meta <code>rtk_saved_bytes</code>).
              </p>
            </section>

            <section class="rounded-lg border border-border bg-surface p-4">
              <h2 class="mb-3 font-medium">Gateway</h2>
              <label class="mb-3 flex items-center justify-between text-sm">
                <span>Require API key on /v1</span>
                <input
                  type="checkbox"
                  checked={s().require_api_key}
                  onChange={() => toggle("require_api_key")}
                />
              </label>
              <label class="mb-3 flex items-center justify-between text-sm">
                <span>Require dashboard login</span>
                <input
                  type="checkbox"
                  checked={s().require_login}
                  onChange={() => toggle("require_login")}
                />
              </label>
              <label class="mb-3 flex items-center justify-between text-sm">
                <span>Request logs</span>
                <input
                  type="checkbox"
                  checked={s().enable_request_logs}
                  onChange={() => toggle("enable_request_logs")}
                />
              </label>
              <label class="flex items-center justify-between text-sm">
                <span>Sticky round-robin limit</span>
                <input
                  type="number"
                  min="1"
                  class="w-20 rounded border border-border bg-bg px-2 py-1 text-sm"
                  value={s().sticky_round_robin_limit}
                  onChange={(e) =>
                    patch({ sticky_round_robin_limit: Number(e.currentTarget.value) || 3 })
                  }
                />
              </label>
            </section>

            <div class="text-sm text-text-muted">
              <Show when={saving()}>saving…</Show>
              {msg()}
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
