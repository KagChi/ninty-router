import { Show, createResource, createSignal } from "solid-js";
import { api } from "~/lib/api";
import { Button, Card, Icon, Input, PageHeader, Select, Toggle, CardSkeleton } from "~/components/ui";

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
  const [importMsg, setImportMsg] = createSignal("");

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

  // Change Password (9router profile page 1:1)
  const [pw, setPw] = createSignal({ current: "", next: "", confirm: "" });
  const [pwMsg, setPwMsg] = createSignal<{ type: "success" | "error"; text: string } | null>(null);
  const changePassword = async (e: Event) => {
    e.preventDefault();
    setPwMsg(null);
    if (pw().next !== pw().confirm) {
      setPwMsg({ type: "error", text: "Passwords do not match" });
      return;
    }
    try {
      await api("/settings", {
        method: "PATCH",
        body: JSON.stringify({ currentPassword: pw().current, newPassword: pw().next }),
      });
      setPwMsg({ type: "success", text: "Password updated successfully" });
      setPw({ current: "", next: "", confirm: "" });
    } catch (err) {
      setPwMsg({ type: "error", text: err instanceof Error ? err.message : "Failed to update password" });
    }
  };
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
      <PageHeader title="Settings" subtitle="Token savers, gateway policy, data import" />
      <Show when={settings()} fallback={<CardSkeleton />}>
        {(s) => (
          <div class="space-y-6">
            <Card title="Token Savers" icon="savings">

              <div class="mb-3 flex items-center justify-between text-sm">
                <span>
                  RTK compression
                  <span class="block text-xs text-text-muted">
                    Compress tool outputs (git diff, grep, ls, build logs…) before sending upstream
                  </span>
                </span>
                <Toggle checked={s().rtk_enabled} onChange={() => toggle("rtk_enabled")} />
              </div>

              <div class="mb-3 flex items-center justify-between text-sm">
                <span>
                  Caveman
                  <span class="block text-xs text-text-muted">
                    Inject terse-response system prompt
                  </span>
                </span>
                <Toggle checked={s().caveman_enabled} onChange={() => toggle("caveman_enabled")} />
              </div>

              <div class="mb-3 flex items-center justify-between text-sm">
                <span>
                  Ponytail
                  <span class="block text-xs text-text-muted">
                    Inject lazy-senior-dev system prompt (minimal code bias)
                  </span>
                </span>
                <Toggle checked={s().ponytail_enabled} onChange={() => toggle("ponytail_enabled")} />
              </div>

              <div class="flex items-center justify-between text-sm">
                <span>Ponytail level</span>
                <Select value={s().ponytail_level} onChange={(v) => patch({ ponytail_level: v })}>
                  <option value="lite">lite</option>
                  <option value="full">full</option>
                  <option value="ultra">ultra</option>
                </Select>
              </div>

              <hr class="my-4 border-border-subtle" />

              <div class="mb-3 flex items-center justify-between text-sm">
                <span>
                  PXPIPE
                  <span class="block text-xs text-text-muted">
                    Render bulky Claude-format context as dense images (cheaper tokens). Requires pxpipe-proxy install + node/bun.
                  </span>
                </span>
                <Toggle checked={s().pxpipe_enabled} onChange={() => toggle("pxpipe_enabled")} />
              </div>

              <Show when={s().pxpipe_enabled}>
                <div class="mb-3 flex items-center justify-between text-sm">
                  <span>Min chars (gate)</span>
                  <div class="w-28">
                    <Input
                      type="number"
                      min="1000"
                      step="1000"
                      value={s().pxpipe_min_chars}
                      onInput={(v) => patch({ pxpipe_min_chars: Number(v) || 25000 })}
                    />
                  </div>
                </div>
                <div class="mb-3 flex items-center justify-between text-sm">
                  <span>Timeout (ms)</span>
                  <div class="w-28">
                    <Input
                      type="number"
                      min="1000"
                      step="1000"
                      value={s().pxpipe_timeout_ms}
                      onInput={(v) => patch({ pxpipe_timeout_ms: Number(v) || 15000 })}
                    />
                  </div>
                </div>
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
                    <Button variant="secondary" size="sm" icon="download" onClick={installPxpipe}>
                      Install pxpipe-proxy
                    </Button>
                  </Show>
                </div>
              </Show>

              <p class="mt-3 text-xs text-text-muted">
                Per-request bypass: header <code>x-9router-token-saver: off</code>. Savings are
                recorded per request (usage meta <code>rtk_saved_bytes</code>).
              </p>
            </Card>

            <Card title="Gateway" icon="lan">
              <div class="mb-3 flex items-center justify-between text-sm">
                <span>Require API key on /v1</span>
                <Toggle checked={s().require_api_key} onChange={() => toggle("require_api_key")} />
              </div>
              <div class="mb-3 flex items-center justify-between text-sm">
                <span>Require dashboard login</span>
                <Toggle checked={s().require_login} onChange={() => toggle("require_login")} />
              </div>
              <div class="mb-3 flex items-center justify-between text-sm">
                <span>Request logs</span>
                <Toggle checked={s().enable_request_logs} onChange={() => toggle("enable_request_logs")} />
              </div>
              <div class="flex items-center justify-between text-sm">
                <span>Sticky round-robin limit</span>
                <div class="w-20">
                  <Input
                    type="number"
                    min="1"
                    value={s().sticky_round_robin_limit}
                    onInput={(v) => patch({ sticky_round_robin_limit: Number(v) || 3 })}
                  />
                </div>
              </div>
            </Card>

            {/* Change Password (9router profile page 1:1) */}
            <Card title="Change Password" icon="lock">
              <form onSubmit={changePassword} class="flex flex-col gap-3">
                <label class="text-xs font-medium text-text-muted">
                  Current password
                  <div class="mt-1">
                    <Input
                      type="password"
                      value={pw().current}
                      onInput={(v) => setPw({ ...pw(), current: v })}
                    />
                  </div>
                </label>
                <label class="text-xs font-medium text-text-muted">
                  New password
                  <div class="mt-1">
                    <Input
                      type="password"
                      value={pw().next}
                      onInput={(v) => setPw({ ...pw(), next: v })}
                    />
                  </div>
                </label>
                <label class="text-xs font-medium text-text-muted">
                  Confirm new password
                  <div class="mt-1">
                    <Input
                      type="password"
                      value={pw().confirm}
                      onInput={(v) => setPw({ ...pw(), confirm: v })}
                    />
                  </div>
                </label>
                <div class="flex items-center gap-3">
                  <Button type="submit" size="sm">
                    Update password
                  </Button>
                  <Show when={pwMsg()}>
                    <span
                      class={`flex items-center gap-1.5 text-sm ${
                        pwMsg()!.type === "success" ? "text-green-500" : "text-danger"
                      }`}
                    >
                      <Icon name={pwMsg()!.type === "success" ? "check_circle" : "error"} class="text-[16px]" />
                      {pwMsg()!.text}
                    </span>
                  </Show>
                </div>
              </form>
            </Card>

            <Card title="Import from 9router" icon="upload">

              <p class="mb-3 text-xs text-text-muted">
                Upload a 9router database export (Settings → Database → Export on your 9router
                instance). Connections, API keys, combos and matching settings are merged —
                existing data is kept, conflicts replaced by id.
              </p>
              <input
                type="file"
                accept="application/json"
                class="text-xs"
                onChange={async (e) => {
                  const file = e.currentTarget.files?.[0];
                  if (!file) return;
                  setImportMsg("");
                  try {
                    const payload = JSON.parse(await file.text());
                    const r = await api<{
                      connections: number;
                      apiKeys: number;
                      combos: number;
                      settingsApplied: string[];
                      skipped: string[];
                    }>("/import/9router", { method: "POST", body: JSON.stringify(payload) });
                    setImportMsg(
                      `imported: ${r.connections} connections, ${r.apiKeys} keys, ${r.combos} combos` +
                        (r.settingsApplied.length
                          ? ` · settings: ${r.settingsApplied.join(", ")}`
                          : "") +
                        (r.skipped.length ? ` · skipped: ${r.skipped.join(", ")}` : "")
                    );
                    await refetch();
                  } catch (err) {
                    setImportMsg(err instanceof Error ? err.message : "import failed");
                  } finally {
                    e.currentTarget.value = "";
                  }
                }}
              />
              <Show when={importMsg()}>
                <p class="mt-2 text-xs text-text-muted">{importMsg()}</p>
              </Show>
            </Card>

            <div class="flex items-center gap-1.5 text-sm text-text-muted">
              <Show when={saving()}>
                <Icon name="progress_activity" class="animate-spin text-[16px]" />
              </Show>
              {msg()}
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
