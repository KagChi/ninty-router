import { For, Show, createResource, createSignal, onCleanup, onMount } from "solid-js";
import { api } from "~/lib/api";

interface QuotaWindow {
  label: string;
  used: number;
  reset_at: string | null;
}

interface QuotaReport {
  connection_id: string;
  provider: string;
  plan: string | null;
  windows: QuotaWindow[];
  error: string | null;
  fetched_at: string;
}

function countdown(resetAt: string | null, now: number): string {
  if (!resetAt) return "";
  const ms = new Date(resetAt).getTime() - now;
  if (ms <= 0) return "resetting…";
  const h = Math.floor(ms / 3_600_000);
  const m = Math.floor((ms % 3_600_000) / 60_000);
  const d = Math.floor(h / 24);
  if (d > 0) return `resets in ${d}d ${h % 24}h`;
  if (h > 0) return `resets in ${h}h ${m}m`;
  return `resets in ${m}m`;
}

function barClass(used: number): string {
  if (used >= 90) return "bg-red-500";
  if (used >= 70) return "bg-amber-500";
  return "bg-primary";
}

export default function QuotaPage() {
  const [now, setNow] = createSignal(Date.now());
  const [reports, { refetch }] = createResource(async () =>
    api<{ reports: QuotaReport[] }>("/usage/quota")
  );

  let timer: ReturnType<typeof setInterval>;
  onMount(() => {
    timer = setInterval(() => setNow(Date.now()), 60_000);
  });
  onCleanup(() => clearInterval(timer));

  return (
    <div>
      <div class="mb-4 flex items-center justify-between">
        <h1 class="text-xl font-semibold">Quota</h1>
        <button
          class="rounded border border-border px-3 py-1 text-sm text-text-muted hover:text-text"
          onClick={() => refetch()}
        >
          refresh
        </button>
      </div>

      <Show when={reports()} fallback={<p class="text-sm text-text-muted">Loading…</p>}>
        <Show
          when={reports()!.reports.length > 0}
          fallback={
            <p class="text-sm text-text-muted">
              No quota-capable connections. Connect Claude, Codex, Copilot, or CodeBuddy to see
              usage.
            </p>
          }
        >
          <div class="grid gap-3 md:grid-cols-2">
            <For each={reports()!.reports}>
              {(r) => (
                <div class="rounded-lg border border-border bg-surface p-4">
                  <div class="mb-2 flex items-center justify-between">
                    <span class="font-medium">{r.provider}</span>
                    <Show when={r.plan}>
                      <span class="rounded bg-bg px-1.5 py-0.5 text-xs text-text-muted">{r.plan}</span>
                    </Show>
                  </div>
                  <Show when={!r.error} fallback={<p class="text-sm text-red-400">{r.error}</p>}>
                    <For each={r.windows}>
                      {(w) => (
                        <div class="mb-3">
                          <div class="mb-1 flex justify-between text-xs text-text-muted">
                            <span>{w.label}</span>
                            <span>
                              {w.used.toFixed(0)}% used
                              <Show when={w.reset_at}>
                                {" · "}
                                {countdown(w.reset_at, now())}
                              </Show>
                            </span>
                          </div>
                          <div class="h-2 overflow-hidden rounded bg-bg">
                            <div
                              class={`h-full ${barClass(w.used)}`}
                              style={{ width: `${Math.min(w.used, 100)}%` }}
                            />
                          </div>
                        </div>
                      )}
                    </For>
                    <p class="text-xs text-text-muted">
                      fetched {new Date(r.fetched_at).toLocaleTimeString()}
                    </p>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Show>
    </div>
  );
}
