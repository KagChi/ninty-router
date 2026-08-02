import { For, Show, createResource, createSignal } from "solid-js";
import { api } from "~/lib/api";

interface Stats {
  total: { requests: number; input: number; output: number };
  today: { requests: number; input: number; output: number };
  rtk_saved_bytes: number;
  by_model: { model: string; requests: number; input: number; output: number }[];
}
interface DayRow {
  day: string;
  requests: number;
  input: number;
  output: number;
}
interface ProviderRow {
  provider: string;
  requests: number;
  input: number;
  output: number;
  errors: number;
}
interface LogRow {
  id: number;
  ts: string;
  provider: string;
  model: string;
  status: string;
  data: {
    input_tokens?: number;
    output_tokens?: number;
    endpoint?: string;
    latencyMs?: number;
    request?: unknown;
    providerRequest?: unknown;
    pxpipe?: { applied?: boolean; savedPct?: number; imageCount?: number };
  };
}

const fmt = (n: number) => n.toLocaleString("en-US");

function downloadCsv(name: string, rows: string[][]) {
  const csv = rows
    .map((r) => r.map((c) => `"${String(c).replaceAll('"', '""')}"`).join(","))
    .join("\n");
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([csv], { type: "text/csv" }));
  a.download = name;
  a.click();
  URL.revokeObjectURL(a.href);
}

export default function UsagePage() {
  const [tab, setTab] = createSignal<"overview" | "logs">("overview");
  const [stats] = createResource(async () => api<Stats>("/usage/stats"));
  const [history] = createResource(async () => api<{ days: DayRow[] }>("/usage/history?days=30"));
  const [providers] = createResource(async () =>
    api<{ providers: ProviderRow[] }>("/usage/providers")
  );
  const [logs] = createResource(async () => api<{ logs: LogRow[] }>("/usage/request-logs"));
  const [open, setOpen] = createSignal<number | null>(null);

  const exportCsv = () => {
    const l = logs()?.logs ?? [];
    downloadCsv(`ninty-usage-${new Date().toISOString().slice(0, 10)}.csv`, [
      ["ts", "provider", "model", "status", "endpoint", "input_tokens", "output_tokens", "latency_ms"],
      ...l.map((r) => [
        r.ts,
        r.provider,
        r.model,
        r.status,
        r.data.endpoint ?? "",
        String(r.data.input_tokens ?? 0),
        String(r.data.output_tokens ?? 0),
        String(r.data.latencyMs ?? ""),
      ]),
    ]);
  };

  const maxTokens = () => Math.max(1, ...(history()?.days ?? []).map((d) => d.input + d.output));

  return (
    <div>
      <div class="mb-4 flex items-center gap-4">
        <h1 class="text-xl font-semibold">Usage</h1>
        <div class="flex gap-1 text-sm">
          <button
            class={`rounded px-3 py-1 ${tab() === "overview" ? "bg-surface font-medium" : "text-text-muted"}`}
            onClick={() => setTab("overview")}
          >
            Overview
          </button>
          <button
            class={`rounded px-3 py-1 ${tab() === "logs" ? "bg-surface font-medium" : "text-text-muted"}`}
            onClick={() => setTab("logs")}
          >
            Request Details
          </button>
        </div>
        <div class="ml-auto">
          <button class="rounded border border-border px-2 py-1 text-xs hover:bg-surface" onClick={exportCsv}>
            Export CSV
          </button>
        </div>
      </div>

      <Show when={tab() === "overview"}>
        <Show when={stats()} fallback={<p class="text-sm text-text-muted">Loading…</p>}>
          {(s) => (
            <div class="space-y-6">
              <div class="grid grid-cols-2 gap-3 md:grid-cols-4">
                <div class="rounded-lg border border-border bg-surface p-3">
                  <div class="text-xs text-text-muted">Requests (today)</div>
                  <div class="text-lg font-semibold">{fmt(s().today.requests)}</div>
                  <div class="text-xs text-text-muted">{fmt(s().total.requests)} total</div>
                </div>
                <div class="rounded-lg border border-border bg-surface p-3">
                  <div class="text-xs text-text-muted">Input tokens (today)</div>
                  <div class="text-lg font-semibold">{fmt(s().today.input)}</div>
                  <div class="text-xs text-text-muted">{fmt(s().total.input)} total</div>
                </div>
                <div class="rounded-lg border border-border bg-surface p-3">
                  <div class="text-xs text-text-muted">Output tokens (today)</div>
                  <div class="text-lg font-semibold">{fmt(s().today.output)}</div>
                  <div class="text-xs text-text-muted">{fmt(s().total.output)} total</div>
                </div>
                <div class="rounded-lg border border-border bg-surface p-3">
                  <div class="text-xs text-text-muted">RTK saved bytes</div>
                  <div class="text-lg font-semibold">{fmt(s().rtk_saved_bytes)}</div>
                  <div class="text-xs text-text-muted">compression savings, not provider-billed</div>
                </div>
              </div>

              <section class="rounded-lg border border-border bg-surface p-4">
                <h2 class="mb-3 text-sm font-medium">Tokens / day (30d)</h2>
                <Show
                  when={(history()?.days.length ?? 0) > 0}
                  fallback={<p class="text-xs text-text-muted">No usage yet.</p>}
                >
                  <div class="flex h-32 items-end gap-1">
                    <For each={history()?.days}>
                      {(d) => (
                        <div
                          class="flex-1 rounded-t bg-accent/70"
                          style={{ height: `${Math.max(2, ((d.input + d.output) / maxTokens()) * 100)}%` }}
                          title={`${d.day}: ${fmt(d.input + d.output)} tokens (${fmt(d.requests)} req)`}
                        />
                      )}
                    </For>
                  </div>
                  <div class="mt-1 flex justify-between text-[10px] text-text-muted">
                    <span>{history()?.days[0]?.day}</span>
                    <span>{history()?.days.at(-1)?.day}</span>
                  </div>
                </Show>
              </section>

              <div class="grid gap-3 md:grid-cols-2">
                <section class="rounded-lg border border-border bg-surface p-4">
                  <h2 class="mb-2 text-sm font-medium">By provider</h2>
                  <table class="w-full text-xs">
                    <thead>
                      <tr class="text-left text-text-muted">
                        <th class="py-1">Provider</th>
                        <th>Requests</th>
                        <th>Input</th>
                        <th>Output</th>
                        <th>Errors</th>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={providers()?.providers}>
                        {(p) => (
                          <tr class="border-t border-border">
                            <td class="py-1 font-medium">{p.provider || "—"}</td>
                            <td>{fmt(p.requests)}</td>
                            <td>{fmt(p.input)}</td>
                            <td>{fmt(p.output)}</td>
                            <td class={p.errors > 0 ? "text-red-400" : ""}>{fmt(p.errors)}</td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </section>

                <section class="rounded-lg border border-border bg-surface p-4">
                  <h2 class="mb-2 text-sm font-medium">By model</h2>
                  <table class="w-full text-xs">
                    <thead>
                      <tr class="text-left text-text-muted">
                        <th class="py-1">Model</th>
                        <th>Requests</th>
                        <th>Input</th>
                        <th>Output</th>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={s().by_model}>
                        {(m) => (
                          <tr class="border-t border-border">
                            <td class="py-1 font-medium">{m.model || "—"}</td>
                            <td>{fmt(m.requests)}</td>
                            <td>{fmt(m.input)}</td>
                            <td>{fmt(m.output)}</td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </section>
              </div>
            </div>
          )}
        </Show>
      </Show>

      <Show when={tab() === "logs"}>
        <Show
          when={(logs()?.logs.length ?? 0) > 0}
          fallback={
            <p class="text-sm text-text-muted">
              No request logs. Enable “Request logs” in Settings first.
            </p>
          }
        >
          <div class="space-y-1">
            <For each={logs()?.logs}>
              {(r) => (
                <div class="rounded border border-border bg-surface text-xs">
                  <button
                    class="flex w-full items-center gap-3 px-3 py-2 text-left hover:bg-bg"
                    onClick={() => setOpen(open() === r.id ? null : r.id)}
                  >
                    <span class="text-text-muted">{r.ts.replace("T", " ").slice(0, 19)}</span>
                    <span class="font-medium">{r.provider}</span>
                    <span class="max-w-48 truncate">{r.model}</span>
                    <span class={r.status === "error" ? "text-red-400" : "text-green-400"}>
                      {r.status}
                    </span>
                    <Show when={r.data.pxpipe?.applied}>
                      <span class="rounded bg-accent/20 px-1">
                        PXPIPE -{r.data.pxpipe?.savedPct}%
                      </span>
                    </Show>
                    <span class="ml-auto text-text-muted">
                      {fmt(r.data.input_tokens ?? 0)}→{fmt(r.data.output_tokens ?? 0)} tok
                      <Show when={r.data.latencyMs}> · {r.data.latencyMs}ms</Show>
                    </span>
                  </button>
                  <Show when={open() === r.id}>
                    <div class="grid gap-2 border-t border-border p-3 md:grid-cols-2">
                      <div>
                        <div class="mb-1 text-text-muted">Client request</div>
                        <pre class="max-h-64 overflow-auto rounded bg-bg p-2">
                          {JSON.stringify(r.data.request, null, 2)}
                        </pre>
                      </div>
                      <div>
                        <div class="mb-1 text-text-muted">Provider request (post-savers)</div>
                        <pre class="max-h-64 overflow-auto rounded bg-bg p-2">
                          {JSON.stringify(r.data.providerRequest, null, 2)}
                        </pre>
                      </div>
                    </div>
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
