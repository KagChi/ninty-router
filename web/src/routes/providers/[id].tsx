import { For, Show, createResource, createSignal } from "solid-js";
import { A, useParams } from "@solidjs/router";
import { api } from "~/lib/api";
import { Badge, Button, Card, CardSection, Icon, Input, Modal, cn, CardSkeleton } from "~/components/ui";
import { ProviderIcon, type Connection, type ModelEntry, type Provider } from "~/components/provider-bits";

/** Capacity badges (9router CAPACITY_META): vision / reasoning. */
function CapacityBadges(props: { caps?: { vision: boolean; reasoning: boolean } }) {
  return (
    <Show when={props.caps && (props.caps.vision || props.caps.reasoning)}>
      <span class="flex items-center gap-0.5">
        <Show when={props.caps?.vision}>
          <Icon name="visibility" class="text-[13px] text-blue-500" />
        </Show>
        <Show when={props.caps?.reasoning}>
          <Icon name="neurology" class="text-[13px] text-amber-500" />
        </Show>
      </span>
    </Show>
  );
}

/** 9router ModelRow chip: status icon + {alias}/{id} mono + name + caps + hover actions. */
function ModelChip(props: {
  provider: string;
  alias: string;
  m: ModelEntry;
  testState: string | undefined;
  copied: boolean;
  onTest: () => void;
  onCopy: () => void;
  onRemove: () => void;
}) {
  const statusIcon = () =>
    props.testState === "ok"
      ? { name: "check_circle", cls: "text-green-500" }
      : props.testState?.startsWith("error")
        ? { name: "cancel", cls: "text-red-500" }
        : { name: "smart_toy", cls: "text-text-muted" };
  return (
    <div
      class="group relative flex items-center gap-2 rounded-[10px] border border-border bg-bg px-2.5 py-1.5"
      title={props.testState ?? props.m.name}
    >
      <Icon
        name={props.testState === "testing…" ? "progress_activity" : statusIcon().name}
        class={`text-[15px] ${statusIcon().cls} ${props.testState === "testing…" ? "animate-spin" : ""}`}
      />
      <code class="font-mono text-xs text-text-main">
        {props.alias}/{props.m.id}
      </code>
      <span class="text-xs italic text-text-muted">{props.m.name !== props.m.id ? props.m.name : ""}</span>
      <CapacityBadges caps={props.m.caps} />
      <Show when={props.m.custom}>
        <Badge tone="blue">custom</Badge>
      </Show>
      <Show when={props.m.suggested}>
        <Badge tone="neutral">free</Badge>
      </Show>
      <div class="ml-1 hidden items-center gap-0.5 group-hover:flex">
        <button class="rounded p-1 text-text-muted hover:bg-surface-2 hover:text-text-main" title="test model" onClick={props.onTest}>
          <Icon name="science" class="text-[14px]" />
        </button>
        <button class="rounded p-1 text-text-muted hover:bg-surface-2 hover:text-text-main" title="copy" onClick={props.onCopy}>
          <Icon name={props.copied ? "check" : "content_copy"} class="text-[14px]" />
        </button>
        <button class="rounded p-1 text-text-muted hover:bg-surface-2 hover:text-danger" title={props.m.custom ? "remove" : "disable"} onClick={props.onRemove}>
          <Icon name="close" class="text-[14px]" />
        </button>
      </div>
    </div>
  );
}

export default function ProviderDetail() {
  const params = useParams();
  // Key on params.id: route modules lazy-load, params populate async — the
  // resource must re-run when the id becomes available.
  const [data, { refetch }] = createResource(
    () => params.id,
    async (id) => {
      if (!id) return undefined;
      const d = await api<{ providers: Provider[] }>("/providers");
      return d.providers.find((p) => p.id === id) ?? null;
    }
  );

  const [adding, setAdding] = createSignal(false);
  const [keyInput, setKeyInput] = createSignal("");
  const [nameInput, setNameInput] = createSignal("");
  const [error, setError] = createSignal("");
  const [testResult, setTestResult] = createSignal<Record<string, string>>({});
  const [oauthOpen, setOauthOpen] = createSignal(false);
  const [oauthInfo, setOauthInfo] = createSignal<{
    url?: string;
    user_code?: string;
    verification_uri?: string;
    state?: string;
  } | null>(null);
  const [oauthCode, setOauthCode] = createSignal("");
  const [oauthStatus, setOauthStatus] = createSignal("");
  const [modelTest, setModelTest] = createSignal<Record<string, string>>({});
  const [addingModel, setAddingModel] = createSignal(false);
  const [newModelId, setNewModelId] = createSignal("");
  const [copied, setCopied] = createSignal("");
  const [selected, setSelected] = createSignal<Set<string>>(new Set());
  const [testingAll, setTestingAll] = createSignal(false);
  const [testAllProgress, setTestAllProgress] = createSignal("");
  const [settings, { refetch: refetchSettings }] = createResource(() =>
    api<{ provider_strategies?: Record<string, { fallback_strategy?: string; sticky_round_robin_limit?: number }> }>("/settings")
  );
  let importInput: HTMLInputElement | undefined;

  const strategy = () => {
    const pid = params.id;
    const s = settings()?.provider_strategies?.[pid];
    return {
      rr: s?.fallback_strategy === "round-robin",
      sticky: s?.sticky_round_robin_limit ?? 3,
    };
  };

  const setStrategy = async (rr: boolean, sticky?: number) => {
    const pid = params.id;
    const cur = settings()?.provider_strategies ?? {};
    await api("/settings", {
      method: "PATCH",
      body: JSON.stringify({
        provider_strategies: {
          ...cur,
          [pid]: { fallback_strategy: rr ? "round-robin" : "priority", sticky_round_robin_limit: sticky ?? strategy().sticky },
        },
      }),
    });
    refetchSettings();
  };

  // ---------- model actions ----------

  const testModel = async (provider: string, model: string) => {
    setModelTest((p) => ({ ...p, [model]: "testing…" }));
    try {
      const r = await api<{ ok: boolean; message?: string; status?: number }>("/models/test", {
        method: "POST",
        body: JSON.stringify({ provider, model }),
      });
      setModelTest((p) => ({ ...p, [model]: r.ok ? "ok" : `error${r.status ? ` ${r.status}` : ""}: ${(r.message ?? "").slice(0, 80)}` }));
    } catch (e) {
      setModelTest((p) => ({ ...p, [model]: `error: ${e instanceof Error ? e.message : "?"}` }));
    }
  };

  const copyModel = (spec: string, id: string) => {
    void navigator.clipboard?.writeText(spec);
    setCopied(id);
    setTimeout(() => setCopied(""), 1500);
  };

  const disableModel = async (provider: string, id: string) => {
    await api("/models/disabled", { method: "POST", body: JSON.stringify({ provider, ids: [id] }) });
    refetch();
  };

  const enableModel = async (provider: string, id: string) => {
    await api(`/models/disabled?provider=${encodeURIComponent(provider)}&id=${encodeURIComponent(id)}`, { method: "DELETE" });
    refetch();
  };

  const disableAll = async (provider: string) => {
    const ids = (data()?.models ?? []).filter((m) => !m.custom).map((m) => m.id);
    if (ids.length === 0) return;
    await api("/models/disabled", { method: "POST", body: JSON.stringify({ provider, ids }) });
    refetch();
  };

  const enableAll = async (provider: string) => {
    await api(`/models/disabled?provider=${encodeURIComponent(provider)}`, { method: "DELETE" });
    refetch();
  };

  const addCustom = async (provider: string) => {
    const id = newModelId().trim();
    if (!id) return;
    await api("/models/custom", { method: "POST", body: JSON.stringify({ provider, id }) });
    setNewModelId("");
    setAddingModel(false);
    refetch();
  };

  const removeCustom = async (provider: string, id: string) => {
    await api(`/models/custom?provider=${encodeURIComponent(provider)}&id=${encodeURIComponent(id)}`, { method: "DELETE" });
    refetch();
  };

  const startOauth = async (provider: string) => {
    setOauthOpen(true);
    setOauthCode("");
    setOauthStatus("");
    setOauthInfo(null);
    try {
      if (provider === "github" || provider === "kiro") {
        const d = await api<{ user_code: string; verification_uri: string; state: string; interval: number }>(
          `/oauth/${provider}/device-code`,
          { method: "POST" }
        );
        setOauthInfo({ user_code: d.user_code, verification_uri: d.verification_uri, state: d.state });
        pollDevice(provider, d.state, Math.max(d.interval, 5));
      } else {
        const d = await api<{ authorize_url: string; state: string }>(`/oauth/${provider}/authorize`, { method: "POST" });
        setOauthInfo({ url: d.authorize_url, state: d.state });
      }
    } catch (e) {
      setOauthStatus(e instanceof Error ? e.message : "failed");
    }
  };

  const pollDevice = (provider: string, state: string, intervalSec: number) => {
    const timer = setInterval(async () => {
      try {
        const d = await api<{ status: string }>(`/oauth/${provider}/poll?state=${state}`);
        if (d.status === "connected") {
          clearInterval(timer);
          setOauthOpen(false);
          setOauthInfo(null);
          refetch();
        } else if (!["pending", "authorization_pending", "slow_down"].includes(d.status)) {
          clearInterval(timer);
          setOauthStatus(`failed: ${d.status}`);
        }
      } catch (e) {
        clearInterval(timer);
        setOauthStatus(e instanceof Error ? e.message : "poll failed");
      }
    }, intervalSec * 1000);
  };

  const submitOauthCode = async () => {
    const provider = params.id;
    if (!oauthCode().trim()) return;
    setOauthStatus("");
    try {
      await api(`/oauth/${provider}/exchange`, {
        method: "POST",
        body: JSON.stringify({ code: oauthCode().trim(), state: oauthInfo()?.state }),
      });
      setOauthOpen(false);
      setOauthInfo(null);
      setOauthCode("");
      refetch();
    } catch (e) {
      setOauthStatus(e instanceof Error ? e.message : "exchange failed");
    }
  };

  const addKey = async () => {
    setError("");
    try {
      await api("/providers", {
        method: "POST",
        body: JSON.stringify({
          provider: params.id,
          api_key: keyInput(),
          name: nameInput() || null,
        }),
      });
      setKeyInput("");
      setNameInput("");
      setAdding(false);
      refetch();
    } catch (e) {
      setError(e instanceof Error ? e.message : "failed");
    }
  };

  const remove = async (id: string) => {
    await api(`/providers/${id}`, { method: "DELETE" });
    refetch();
  };

  const toggle = async (id: string, active: boolean) => {
    await api(`/providers/${id}`, { method: "PUT", body: JSON.stringify({ is_active: active }) });
    refetch();
  };

  const test = async (id: string) => {
    setTestResult((p) => ({ ...p, [id]: "testing…" }));
    try {
      const res = await api<{ ok: boolean; message: string }>(`/providers/${id}/test`, {
        method: "POST",
      });
      setTestResult((p) => ({ ...p, [id]: res.ok ? "ok" : `error: ${res.message.slice(0, 80)}` }));
    } catch (e) {
      setTestResult((p) => ({ ...p, [id]: `error: ${e instanceof Error ? e.message : "?"}` }));
    }
  };

  const connLabel = (c: Connection) =>
    c.name ?? (c.data.email as string) ?? c.data.apiKey ?? c.id.slice(0, 8);

  /** Swap priority with adjacent conn (9router up/down arrows). */
  const movePriority = async (c: Connection, dir: -1 | 1) => {
    const list = [...(data()?.connections ?? [])].sort((a, b) => a.priority - b.priority);
    const i = list.findIndex((x) => x.id === c.id);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= list.length) return;
    await api(`/providers/${list[i].id}`, { method: "PUT", body: JSON.stringify({ priority: list[j].priority }) });
    await api(`/providers/${list[j].id}`, { method: "PUT", body: JSON.stringify({ priority: list[i].priority }) });
    refetch();
  };

  /** Test all connections one-by-one (9router "Test 1 by 1"). */
  const testOneByOne = async () => {
    const list = data()?.connections ?? [];
    setTestingAll(true);
    for (let i = 0; i < list.length; i++) {
      setTestAllProgress(`${i + 1}/${list.length}`);
      await test(list[i].id);
    }
    setTestingAll(false);
    setTestAllProgress("");
    refetch();
  };

  const exportConns = async () => {
    const d = await api<{ connections: unknown[] }>(`/providers/export/${params.id}`);
    const blob = new Blob([JSON.stringify(d, null, 2)], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `${params.id}-connections.json`;
    a.click();
    URL.revokeObjectURL(a.href);
  };

  const importConns = async (file: File) => {
    try {
      const text = JSON.parse(await file.text());
      const r = await api<{ created: number }>(`/providers/import/${params.id}`, {
        method: "POST",
        body: JSON.stringify(text),
      });
      setError(r.created > 0 ? "" : "nothing imported (dupes or empty file)");
      refetch();
    } catch (e) {
      setError(e instanceof Error ? e.message : "import failed");
    }
  };

  const toggleSelected = (id: string, on: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  const selectAll = (on: boolean) =>
    setSelected(on ? new Set((data()?.connections ?? []).map((c) => c.id)) : new Set());

  const bulk = async (op: "enable" | "disable" | "delete") => {
    const ids = [...selected()];
    for (const id of ids) {
      if (op === "delete") await api(`/providers/${id}`, { method: "DELETE" });
      else await api(`/providers/${id}`, { method: "PUT", body: JSON.stringify({ is_active: op === "enable" }) });
    }
    setSelected(new Set<string>());
    refetch();
  };

  return (
    <div class="flex flex-col gap-6">
      <Show
        when={data() !== undefined}
        fallback={<><CardSkeleton /><CardSkeleton /></>}
      >
        <Show
          when={data()}
          fallback={
            <Card>
              <p class="py-4 text-center text-sm text-text-muted">
                Provider “{params.id}” not found.
              </p>
            </Card>
          }
        >
        {(p) => (
          <>
            <div class="flex flex-wrap items-center gap-3 sm:gap-4">
              <A
                href="/providers"
                class="rounded-lg p-1.5 text-text-muted no-underline hover:bg-surface-2 hover:text-text-main"
              >
                <Icon name="arrow_back" class="text-[20px]" />
              </A>
              <ProviderIcon id={p().id} color={p().color} textIcon={p().text_icon} size={40} />
              <div class="min-w-0 flex-1 basis-40">
                <h1 class="text-2xl font-semibold tracking-tight text-text-main">
                  {p().display_name}
                </h1>
                <p class="truncate text-sm text-text-muted">
                  <code class="font-mono">{p().id}/model</code>
                  {" · alias "}
                  <code class="font-mono">{p().alias}/model</code>
                </p>
              </div>
              <div class="flex items-center gap-3 max-sm:ml-auto">
                <Show when={p().notice_url}>
                  <a
                    href={p().notice_url!}
                    target="_blank"
                    rel="noreferrer"
                    class="text-sm text-brand-500 hover:underline"
                  >
                    get key ↗
                  </a>
                </Show>
                <Show when={p().category === "oauth"}>
                  <Button size="sm" icon="link" onClick={() => startOauth(p().id)}>
                    Connect
                  </Button>
                </Show>
                <Button variant="secondary" size="sm" icon="add" onClick={() => setAdding(!adding())}>
                  Add key
                </Button>
              </div>
            </div>

            {/* 9router deprecation banner (yellow, RISK_NOTICE) */}
            <Show when={p().deprecated}>
              <div class="flex items-start gap-2 rounded-lg border border-yellow-500/30 bg-yellow-500/10 px-3 py-2">
                <Icon name="warning" class="mt-0.5 shrink-0 text-[16px] text-yellow-500" />
                <p class="text-xs leading-relaxed text-yellow-600 dark:text-yellow-400">
                  {p().deprecation_notice}
                </p>
              </div>
            </Show>

            <Show when={error()}>
              <p class="flex items-center gap-1.5 text-sm text-danger">
                <Icon name="error" class="text-[16px]" /> {error()}
              </p>
            </Show>

            <Show when={adding()}>
              <CardSection class="flex gap-2">
                <div class="flex-1">
                  <Input placeholder="api key" value={keyInput()} onInput={setKeyInput} />
                </div>
                <div class="w-40">
                  <Input placeholder="name (opt)" value={nameInput()} onInput={setNameInput} />
                </div>
                <Button size="md" onClick={addKey}>
                  Save
                </Button>
              </CardSection>
            </Show>

            <Card
              title={`Connections (${p().connections.length})`}
              icon="dns"
              action={
                <div class="flex flex-wrap items-center gap-3">
                  {/* Round Robin toggle + sticky count */}
                  <label class="flex cursor-pointer items-center gap-1.5 text-xs text-text-muted">
                    <input
                      type="checkbox"
                      class="accent-brand-500"
                      checked={strategy().rr}
                      onChange={(e) => setStrategy(e.currentTarget.checked)}
                    />
                    Round Robin
                  </label>
                  <Show when={strategy().rr}>
                    <label class="flex items-center gap-1 text-xs text-text-muted">
                      sticky
                      <input
                        type="number"
                        min="1"
                        class="w-12 rounded-md border border-border bg-bg px-1.5 py-0.5 text-xs text-text-main"
                        value={strategy().sticky}
                        onChange={(e) => setStrategy(true, Number(e.currentTarget.value) || 1)}
                      />
                    </label>
                  </Show>
                  <button class="rounded p-1 text-text-muted hover:bg-surface-2 hover:text-text-main" title="import connections" onClick={() => importInput?.click()}>
                    <Icon name="upload" class="text-[16px]" />
                  </button>
                  <button class="rounded p-1 text-text-muted hover:bg-surface-2 hover:text-text-main" title="export connections" onClick={exportConns}>
                    <Icon name="download" class="text-[16px]" />
                  </button>
                  <Button variant="ghost" size="sm" icon="network_check" disabled={testingAll() || p().connections.length === 0} onClick={testOneByOne}>
                    {testingAll() ? `Testing ${testAllProgress()}` : "Test 1 by 1"}
                  </Button>
                  <input
                    ref={importInput}
                    type="file"
                    accept="application/json"
                    class="hidden"
                    onChange={(e) => {
                      const f = e.currentTarget.files?.[0];
                      if (f) void importConns(f);
                      e.currentTarget.value = "";
                    }}
                  />
                </div>
              }
            >
              <Show
                when={p().connections.length > 0}
                fallback={
                  <p class="py-4 text-center text-sm text-text-muted">
                    No connections yet — add an API key{p().category === "oauth" ? " or connect via OAuth" : ""}.
                  </p>
                }
              >
                {/* bulk bar */}
                <Show when={selected().size > 0}>
                  <div class="mb-2 flex items-center gap-2 rounded-[10px] border border-brand-500/30 bg-brand-500/5 px-3 py-1.5 text-xs">
                    <span class="text-text-muted">{selected().size} selected</span>
                    <Button variant="ghost" size="sm" onClick={() => bulk("enable")}>enable</Button>
                    <Button variant="ghost" size="sm" onClick={() => bulk("disable")}>disable</Button>
                    <Button variant="ghost" size="sm" class="text-danger" onClick={() => bulk("delete")}>delete</Button>
                  </div>
                </Show>
                <div class="overflow-x-auto">
                  <table class="w-full text-sm">
                    <thead>
                      <tr class="border-b border-border-subtle text-left text-xs font-semibold text-text-muted">
                        <th class="w-8 py-2 pr-2">
                          <input
                            type="checkbox"
                            class="accent-brand-500"
                            checked={selected().size === p().connections.length && p().connections.length > 0}
                            onChange={(e) => selectAll(e.currentTarget.checked)}
                          />
                        </th>
                        <th class="w-10 py-2 pr-2" title="priority" />
                        <th class="py-2 pr-3">Account</th>
                        <th class="py-2 pr-3">Status</th>
                        <th class="py-2 pr-3">Test</th>
                        <th class="py-2" />
                      </tr>
                    </thead>
                    <tbody>
                      <For each={[...p().connections].sort((a, b) => a.priority - b.priority)}>
                        {(c) => {
                          const authIcon = () =>
                            c.auth_type === "oauth" ? "key" : c.data.apiKey ? "password" : "link";
                          const errMsg = () => (c.data.lastError as string) ?? "";
                          return (
                            <tr class="border-b border-border-subtle last:border-b-0">
                              <td class="py-2.5 pr-2">
                                <input
                                  type="checkbox"
                                  class="accent-brand-500"
                                  checked={selected().has(c.id)}
                                  onChange={(e) => toggleSelected(c.id, e.currentTarget.checked)}
                                />
                              </td>
                              <td class="py-2.5 pr-2">
                                <div class="flex flex-col">
                                  <button class="text-text-muted hover:text-brand-500 disabled:opacity-30" disabled={c.priority <= 1} onClick={() => movePriority(c, -1)} title="priority up">
                                    <Icon name="keyboard_arrow_up" class="text-[16px]" />
                                  </button>
                                  <button class="text-text-muted hover:text-brand-500" onClick={() => movePriority(c, 1)} title="priority down">
                                    <Icon name="keyboard_arrow_down" class="text-[16px]" />
                                  </button>
                                </div>
                              </td>
                              <td class="max-w-56 truncate py-2.5 pr-3">
                                <span class="flex items-center gap-1.5">
                                  <Icon name={authIcon()} class="text-[14px] text-text-muted" />
                                  <span class="truncate font-medium">{connLabel(c)}</span>
                                </span>
                                <Show when={errMsg()}>
                                  <span class="block truncate text-[11px] text-danger" title={errMsg()}>
                                    {errMsg()}
                                  </span>
                                </Show>
                              </td>
                              <td class="py-2.5 pr-3">
                                <Badge tone={c.is_active ? "green" : "neutral"}>
                                  {c.is_active ? "active" : "disabled"}
                                </Badge>
                              </td>
                              <td class="py-2.5 pr-3 text-xs text-text-muted">
                                {testResult()[c.id] ?? (c.data.testStatus === "ok" ? "active" : c.data.testStatus) ?? ""}
                              </td>
                              <td class="py-2.5 text-right">
                                <Button variant="ghost" size="sm" onClick={() => test(c.id)}>
                                  test
                                </Button>
                                <Button variant="ghost" size="sm" onClick={() => toggle(c.id, !c.is_active)}>
                                  {c.is_active ? "disable" : "enable"}
                                </Button>
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  class="text-danger"
                                  onClick={() => remove(c.id)}
                                >
                                  delete
                                </Button>
                              </td>
                            </tr>
                          );
                        }}
                      </For>
                    </tbody>
                  </table>
                </div>
              </Show>
            </Card>

            <Card
              title={`Available Models (${p().models.filter((m) => !m.disabled).length})`}
              icon="model_training"
              action={
                <div class="flex gap-1">
                  <Button variant="ghost" size="sm" icon="restart_alt" onClick={() => enableAll(p().id)}>
                    Active All
                  </Button>
                  <Button variant="ghost" size="sm" icon="block" onClick={() => disableAll(p().id)}>
                    Disable All
                  </Button>
                </div>
              }
            >
              <div class="flex flex-wrap gap-3">
                <For each={p().models.filter((m) => !m.disabled)}>
                  {(m) => (
                    <ModelChip
                      provider={p().id}
                      alias={p().alias}
                      m={m}
                      testState={modelTest()[m.id]}
                      onTest={() => testModel(p().id, m.id)}
                      onCopy={() => copyModel(`${p().alias}/${m.id}`, m.id)}
                      copied={copied() === m.id}
                      onRemove={() => (m.custom ? removeCustom(p().id, m.id) : disableModel(p().id, m.id))}
                    />
                  )}
                </For>

                {/* inline Add Model */}
                <Show
                  when={!addingModel()}
                  fallback={
                    <div class="flex items-center gap-1.5 rounded-[10px] border border-brand-500/50 px-2 py-1.5">
                      <input
                        class="w-40 bg-transparent font-mono text-xs text-text-main placeholder:text-text-subtle focus:outline-none"
                        placeholder="model-id"
                        value={newModelId()}
                        onInput={(e) => setNewModelId(e.currentTarget.value)}
                        onKeyDown={(e) => e.key === "Enter" && addCustom(p().id)}
                      />
                      <button class="text-brand-500 hover:underline text-xs" onClick={() => addCustom(p().id)}>add</button>
                      <button class="text-text-muted text-xs" onClick={() => setAddingModel(false)}>✕</button>
                    </div>
                  }
                >
                  <button
                    class="flex items-center gap-1 rounded-[10px] border border-dashed border-border px-2.5 py-1.5 text-xs text-text-muted transition-colors hover:border-brand-500/50 hover:text-brand-500"
                    onClick={() => setAddingModel(true)}
                  >
                    <Icon name="add" class="text-[14px]" /> Add Model
                  </button>
                </Show>
              </div>

              {/* disabled models block */}
              <Show when={p().models.some((m) => m.disabled)}>
                <div class="mt-4 border-t border-border-subtle pt-3">
                  <p class="mb-2 text-xs font-semibold text-text-muted">
                    Disabled models ({p().models.filter((m) => m.disabled).length}):
                  </p>
                  <div class="flex flex-wrap gap-2">
                    <For each={p().models.filter((m) => m.disabled)}>
                      {(m) => (
                        <button
                          class="flex items-center gap-1.5 rounded-[10px] border border-border bg-bg px-2.5 py-1.5 font-mono text-xs text-text-muted opacity-70 transition-colors hover:border-green-500/50 hover:text-green-500"
                          title="restore"
                          onClick={() => enableModel(p().id, m.id)}
                        >
                          <Icon name="restart_alt" class="text-[14px]" /> {m.id}
                        </button>
                      )}
                    </For>
                  </div>
                </div>
              </Show>
            </Card>

            <Modal
              open={oauthOpen()}
              title={`Connect ${p().display_name}`}
              onClose={() => {
                setOauthOpen(false);
                setOauthInfo(null);
              }}
            >
              <Show when={oauthInfo()?.url}>
                <a
                  href={oauthInfo()!.url}
                  target="_blank"
                  rel="noreferrer"
                  class="text-sm text-brand-500 hover:underline"
                >
                  open authorize page ↗
                </a>
                <p class="mt-1 text-xs text-text-muted">Sign in, then paste the code below.</p>
                <div class="mt-2 flex gap-2">
                  <div class="flex-1">
                    <Input
                      placeholder="paste code (code#state)"
                      value={oauthCode()}
                      onInput={setOauthCode}
                    />
                  </div>
                  <Button size="md" onClick={submitOauthCode}>
                    exchange
                  </Button>
                </div>
              </Show>
              <Show when={oauthInfo()?.user_code}>
                <p class="text-sm">
                  Go to{" "}
                  <a
                    href={oauthInfo()!.verification_uri}
                    target="_blank"
                    rel="noreferrer"
                    class="text-brand-500 hover:underline"
                  >
                    {oauthInfo()!.verification_uri}
                  </a>{" "}
                  and enter code:
                </p>
                <p class="my-2 font-mono text-lg tracking-widest">{oauthInfo()!.user_code}</p>
                <p class="flex items-center gap-1.5 text-xs text-text-muted">
                  <Icon name="progress_activity" class="animate-spin text-[14px]" />
                  waiting for confirmation…
                </p>
              </Show>
              <Show when={!oauthInfo()}>
                <p class="flex items-center gap-1.5 text-sm text-text-muted">
                  <Icon name="progress_activity" class="animate-spin text-[16px]" /> starting…
                </p>
              </Show>
              <Show when={oauthStatus()}>
                <p class="mt-2 flex items-center gap-1.5 text-sm text-danger">
                  <Icon name="error" class="text-[16px]" /> {oauthStatus()}
                </p>
              </Show>
            </Modal>
          </>
        )}
        </Show>
      </Show>
    </div>
  );
}
