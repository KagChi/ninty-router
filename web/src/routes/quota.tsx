import { For, Show, createMemo, createResource, createSignal, onCleanup, onMount } from "solid-js";
import { api } from "~/lib/api";
import { Badge, Button, Card, Icon, PageHeader, Select, CardSkeleton } from "~/components/ui";
import { ProviderIcon } from "~/components/provider-bits";

// ---------- types (backend QuotaReport) ----------

interface QuotaWindow {
  label: string;
  used: number;
  total: number;
  unlimited: boolean;
  remaining: number | null;
  recurring: boolean;
  reset_at: string | null;
}

interface QuotaReport {
  connection_id: string;
  provider: string;
  plan: string | null;
  windows: QuotaWindow[];
  error: string | null;
  label: string | null;
  secondary: string | null;
  active: boolean;
  priority: number;
  fetched_at: string;
}

interface ProviderDef {
  id: string;
  display_name: string;
  color: string;
  text_icon: string;
}

// ---------- helpers (port of ProviderLimits/utils.js) ----------

const DEPLETED_THRESHOLD = 5;
const REFRESH_MS = 60_000;

/** Remaining % — 9router calculatePercentage (total 0 → 0, no used → 100). */
function remainingPct(w: QuotaWindow): number {
  if (w.remaining != null) return Math.max(0, Math.round(w.remaining));
  if (w.unlimited || !w.total) return 100;
  if (!w.used || w.used < 0) return 100;
  if (w.used >= w.total) return 0;
  return Math.round(((w.total - w.used) / w.total) * 100);
}

function colorOf(pct: number): { text: string; bg: string; bgLight: string; emoji: string } {
  if (pct > 70) return { text: "text-green-500", bg: "bg-green-500", bgLight: "bg-green-500/10", emoji: "🟢" };
  if (pct >= 30) return { text: "text-yellow-500", bg: "bg-yellow-500", bgLight: "bg-yellow-500/10", emoji: "🟡" };
  return { text: "text-red-500", bg: "bg-red-500", bgLight: "bg-red-500/10", emoji: "🔴" };
}

/** Countdown like 9router formatResetTime: "3m", "2h 5m", "1d 3h 20m" ("-" past). */
function countdown(resetAt: string | null, now: number): string {
  if (!resetAt) return "-";
  const diff = new Date(resetAt).getTime() - now;
  if (diff <= 0) return "-";
  const totalMin = Math.ceil(diff / 60_000);
  if (totalMin < 60) return `${totalMin}m`;
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (h < 24) return `${h}h ${m}m`;
  const d = Math.floor(h / 24);
  return `${d}d ${h % 24}h ${m}m`;
}

/** "Today, 3:00 PM" / "Tomorrow, 3:00 PM" / "Aug 5, 3:00 PM". */
function resetDisplay(resetAt: string | null): string | null {
  if (!resetAt) return null;
  const d = new Date(resetAt);
  if (Number.isNaN(d.getTime())) return null;
  const now = new Date();
  const tomorrow = new Date(now.getTime() + 86_400_000);
  const time = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: true });
  if (d.toDateString() === now.toDateString()) return `Today, ${time}`;
  if (d.toDateString() === tomorrow.toDateString()) return `Tomorrow, ${time}`;
  return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: true });
}

function isDepleted(r: QuotaReport): boolean {
  return r.windows.some((w) => !w.unlimited && w.total > 0 && remainingPct(w) <= DEPLETED_THRESHOLD);
}

const fmtNum = (n: number) => (Number.isInteger(n) ? n.toLocaleString() : n.toLocaleString(undefined, { maximumFractionDigits: 2 }));

// ---------- quota row (QuotaTable row) ----------

function QuotaRow(props: { w: QuotaWindow; now: number }) {
  const pct = () => remainingPct(props.w);
  const c = () => colorOf(pct());
  const word = () => (props.w.recurring ? "in" : "expires in");
  const cd = () => countdown(props.w.reset_at, props.now);
  const disp = () => resetDisplay(props.w.reset_at);
  return (
    <div class="flex items-center gap-3 py-1.5">
      <span class="text-xs">{c().emoji}</span>
      <span class="w-36 shrink-0 truncate text-sm font-medium text-text-main" title={props.w.label}>
        {props.w.label}
      </span>
      <div class="min-w-0 flex-1">
        <div class={`h-1 overflow-hidden rounded-full ${c().bgLight}`}>
          <div class={`h-full ${c().bg}`} style={{ width: `${Math.min(pct(), 100)}%` }} />
        </div>
      </div>
      <span class="shrink-0 text-xs text-text-muted">
        {fmtNum(props.w.used)} / {props.w.unlimited || props.w.total <= 0 ? "∞" : fmtNum(props.w.total)}
        {" · "}
        <span class={c().text}>{pct()}%</span>
      </span>
      <span class="w-40 shrink-0 text-right text-xs text-text-muted" title={disp() ?? undefined}>
        {cd() !== "-" ? `${word()} ${cd()}` : "N/A"}
      </span>
    </div>
  );
}

// ---------- card (ProviderLimitCard) ----------

function QuotaCard(props: {
  r: QuotaReport;
  def: ProviderDef | undefined;
  now: number;
  refreshing: boolean;
  onRefresh: () => void;
}) {
  const color = () => props.def?.color ?? "#6B7280";
  const textIcon = () => props.def?.text_icon ?? props.r.provider.slice(0, 2).toUpperCase();
  return (
    <Card padding="sm" class={!props.r.active ? "opacity-60" : isDepleted(props.r) ? "ring-1 ring-red-500/40" : ""}>
      <div class="mb-3 flex items-center justify-between">
        <div class="flex min-w-0 items-center gap-3">
          <ProviderIcon id={props.r.provider} color={color()} textIcon={textIcon()} size={40} />
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <h3 class="truncate font-semibold text-text-main">
                {props.r.label ?? props.def?.display_name ?? props.r.provider}
              </h3>
              <Show when={props.r.plan}>
                <Badge tone="brand">{props.r.plan}</Badge>
              </Show>
              <Show when={!props.r.active}>
                <Badge tone="amber">disabled</Badge>
              </Show>
            </div>
            <Show when={props.r.secondary}>
              <p class="truncate text-xs text-text-muted">{props.r.secondary}</p>
            </Show>
          </div>
        </div>
        <button
          class="rounded-lg p-2 transition-colors hover:bg-black/5 disabled:opacity-50 dark:hover:bg-white/5"
          disabled={props.refreshing}
          onClick={props.onRefresh}
          title="Refresh quota"
        >
          <Icon name="refresh" class={`text-[20px] text-text-muted ${props.refreshing ? "animate-spin" : ""}`} />
        </button>
      </div>

      <Show
        when={!props.r.error}
        fallback={
          <div class="rounded-lg border border-red-500/20 bg-red-500/10 p-3">
            <div class="flex items-start gap-2">
              <Icon name="error" class="text-[18px] text-red-500" />
              <p class="text-sm text-red-600 dark:text-red-400">{props.r.error}</p>
            </div>
          </div>
        }
      >
        <Show
          when={props.r.windows.length > 0}
          fallback={
            <div class="py-6 text-center text-text-muted">
              <Icon name="data_usage" class="text-[40px] opacity-20" />
              <p class="mt-1 text-sm">No quota data available</p>
            </div>
          }
        >
          <div class="divide-y divide-border-subtle">
            <For each={props.r.windows}>{(w) => <QuotaRow w={w} now={props.now} />}</For>
          </div>
          <p class="mt-2 text-[11px] text-text-muted/70">
            fetched {new Date(props.r.fetched_at).toLocaleTimeString()}
          </p>
        </Show>
      </Show>
    </Card>
  );
}

// ---------- page ----------

const PAGE_SIZES = [10, 20, 50, 100];

export default function QuotaPage() {
  const [now, setNow] = createSignal(Date.now());
  const [reports, { refetch, mutate }] = createResource(async () =>
    api<{ reports: QuotaReport[] }>("/usage/quota")
  );
  const [defs] = createResource(async () => {
    const d = await api<{ providers: ProviderDef[] }>("/providers");
    return new Map(d.providers.map((p) => [p.id, p]));
  });

  const [autoRefresh, setAutoRefresh] = createSignal(localStorage.getItem("quotaAutoRefresh") !== "off");
  const [refreshing, setRefreshing] = createSignal<Record<string, boolean>>({});
  const [accountFilter, setAccountFilter] = createSignal<"all" | "active" | "inactive">("all");
  const [providerFilter, setProviderFilter] = createSignal("");
  const [sort, setSort] = createSignal<"priority" | "expiringFirst">("priority");
  const [page, setPage] = createSignal(1);
  const [pageSize, setPageSize] = createSignal(20);

  // Per-card refresh (live endpoint).
  const refreshOne = async (id: string) => {
    setRefreshing((p) => ({ ...p, [id]: true }));
    try {
      const r = await api<QuotaReport>(`/usage/quota/${id}`);
      const cur = reports();
      if (cur) {
        mutate({ reports: cur.reports.map((x) => (x.connection_id === id ? r : x)) });
      }
    } finally {
      setRefreshing((p) => ({ ...p, [id]: false }));
    }
  };

  // Auto-refresh: 60s tick, paused when tab hidden (list endpoint is 5min-cached server-side).
  let timer: ReturnType<typeof setInterval> | undefined;
  onMount(() => {
    timer = setInterval(() => {
      if (document.hidden || !autoRefresh()) return;
      void refetch();
    }, REFRESH_MS);
  });
  onCleanup(() => timer && clearInterval(timer));
  const clock = setInterval(() => setNow(Date.now()), 60_000);
  onCleanup(() => clearInterval(clock));

  const filtered = createMemo(() => {
    let list = reports()?.reports ?? [];
    if (accountFilter() === "active") list = list.filter((r) => r.active);
    if (accountFilter() === "inactive") list = list.filter((r) => !r.active);
    const pf = providerFilter();
    if (pf) list = list.filter((r) => r.provider === pf);
    if (sort() === "expiringFirst") {
      const soonest = (r: QuotaReport) =>
        Math.min(...r.windows.map((w) => (w.reset_at ? new Date(w.reset_at).getTime() : Infinity)), Infinity);
      list = [...list].sort((a, b) => soonest(a) - soonest(b));
    } else {
      list = [...list].sort((a, b) => a.priority - b.priority || a.provider.localeCompare(b.provider));
    }
    return list;
  });

  const providersInList = createMemo(() => {
    const set = new Set((reports()?.reports ?? []).map((r) => r.provider));
    // 9router dropdown excludes codebuddy (cards still render).
    set.delete("codebuddy-cn");
    set.delete("codebuddy-intl");
    return [...set].sort();
  });

  const totalPages = createMemo(() => Math.max(1, Math.ceil(filtered().length / pageSize())));
  const pageItems = createMemo(() => {
    const p = Math.min(page(), totalPages());
    return filtered().slice((p - 1) * pageSize(), p * pageSize());
  });

  return (
    <div>
      <PageHeader
        title="Quota Tracker"
        subtitle="Track and manage your API quota limits"
        actions={
          <div class="flex flex-wrap items-center gap-2">
            <label class="flex cursor-pointer items-center gap-1.5 text-xs text-text-muted">
              <input
                type="checkbox"
                checked={autoRefresh()}
                onChange={(e) => {
                  setAutoRefresh(e.currentTarget.checked);
                  localStorage.setItem("quotaAutoRefresh", e.currentTarget.checked ? "on" : "off");
                }}
              />
              auto (60s)
            </label>
            <Select value={accountFilter()} onChange={(v) => { setAccountFilter(v as never); setPage(1); }}>
              <option value="all">all accounts</option>
              <option value="active">active</option>
              <option value="inactive">inactive</option>
            </Select>
            <Select value={providerFilter()} onChange={(v) => { setProviderFilter(v); setPage(1); }}>
              <option value="">all providers</option>
              <For each={providersInList()}>{(p) => <option value={p}>{defs()?.get(p)?.display_name ?? p}</option>}</For>
            </Select>
            <Select value={sort()} onChange={(v) => setSort(v as never)}>
              <option value="priority">priority</option>
              <option value="expiringFirst">expiring first</option>
            </Select>
            <Button variant="secondary" size="sm" icon="refresh" onClick={() => refetch()}>
              refresh all
            </Button>
          </div>
        }
      />

      <Show
        when={reports()}
        fallback={
          <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
            <CardSkeleton />
            <CardSkeleton />
          </div>
        }
      >
        <Show
          when={(reports()!.reports.length ?? 0) > 0}
          fallback={
            <Card>
              <div class="py-10 text-center text-text-muted">
                <Icon name="cloud_off" class="text-[48px] opacity-20" />
                <p class="mt-2 font-semibold text-text-main">No Providers Connected</p>
                <p class="mt-1 text-sm">Connect providers with OAuth or API keys to track quota.</p>
              </div>
            </Card>
          }
        >
          <Show
            when={filtered().length > 0}
            fallback={
              <Card>
                <p class="py-8 text-center text-sm text-text-muted">No connections match the current filters.</p>
              </Card>
            }
          >
            <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
              <For each={pageItems()}>
                {(r) => (
                  <QuotaCard
                    r={r}
                    def={defs()?.get(r.provider)}
                    now={now()}
                    refreshing={!!refreshing()[r.connection_id]}
                    onRefresh={() => refreshOne(r.connection_id)}
                  />
                )}
              </For>
            </div>

            {/* pagination */}
            <div class="mt-4 flex flex-wrap items-center justify-between gap-2 text-xs text-text-muted">
              <span>
                Showing {(Math.min(page(), totalPages()) - 1) * pageSize() + 1}–
                {Math.min(page() * pageSize(), filtered().length)} of {filtered().length}
              </span>
              <div class="flex items-center gap-1">
                <Select
                  value={String(pageSize())}
                  onChange={(v) => { setPageSize(Number(v)); setPage(1); }}
                >
                  <For each={PAGE_SIZES}>{(s) => <option value={s}>{s} / page</option>}</For>
                </Select>
                <Button variant="secondary" size="sm" disabled={page() <= 1} onClick={() => setPage(1)}>First</Button>
                <Button variant="secondary" size="sm" disabled={page() <= 1} onClick={() => setPage(page() - 1)}>Prev</Button>
                <Button variant="secondary" size="sm" disabled={page() >= totalPages()} onClick={() => setPage(page() + 1)}>Next</Button>
                <Button variant="secondary" size="sm" disabled={page() >= totalPages()} onClick={() => setPage(totalPages())}>Last</Button>
              </div>
            </div>
          </Show>
        </Show>
      </Show>
    </div>
  );
}
