import { Show, createResource, createSignal } from "solid-js";
import { api } from "~/lib/api";

interface Settings {
  rtk_enabled: boolean;
  caveman_enabled: boolean;
  ponytail_enabled: boolean;
  ponytail_level: string;
  require_api_key: boolean;
  require_login: boolean;
  enable_request_logs: boolean;
  sticky_round_robin_limit: number;
  [k: string]: unknown;
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
