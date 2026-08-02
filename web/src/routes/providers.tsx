import { For, Show, createResource, createSignal } from "solid-js";
import { A } from "@solidjs/router";
import { api } from "~/lib/api";
import { Badge, Button, Card, Icon, Input, Modal, PageHeader, Toggle, cn } from "~/components/ui";

export interface Connection {
  id: string;
  provider: string;
  auth_type: string;
  name: string | null;
  priority: number;
  is_active: boolean;
  data: Record<string, unknown> & {
    apiKey?: string;
    testStatus?: string;
    lastError?: string;
    unavailableUntil?: string;
  };
}

export interface Provider {
  id: string;
  alias: string;
  category: string;
  display_name: string;
  notice_url: string | null;
  color: string;
  text_icon: string;
  no_auth: boolean;
  models: { id: string; name: string }[];
  connections: Connection[];
}

export interface Node {
  id: string;
  name: string | null;
  data: { prefix?: string; baseUrl?: string; apiKey?: string };
}

export function ProviderIcon(props: { id: string; color: string; textIcon: string; size?: number }) {
  const size = () => props.size ?? 32;
  const [err, setErr] = createSignal(false);
  return (
    <div
      class="flex shrink-0 items-center justify-center rounded-lg"
      style={{
        width: `${size()}px`,
        height: `${size()}px`,
        "background-color": `${props.color}${props.color.length > 7 ? "" : "15"}`,
      }}
    >
      <Show
        when={!err()}
        fallback={
          <span class="font-bold" style={{ color: props.color, "font-size": `${Math.max(10, size() * 0.38)}px` }}>
            {props.textIcon}
          </span>
        }
      >
        <img
          src={`/providers/${props.id}.png`}
          alt={props.id}
          width={size() - 2}
          height={size() - 2}
          class="max-w-full rounded-lg object-contain"
          loading="lazy"
          onError={() => setErr(true)}
        />
      </Show>
    </div>
  );
}

function stats(p: Provider) {
  const total = p.connections.length;
  const connected = p.connections.filter((c) => c.is_active).length;
  const error = p.connections.filter((c) => c.data.lastError).length;
  const allDisabled = total > 0 && connected === 0;
  return { total, connected, error, allDisabled };
}

function StatusBadges(p: Provider) {
  const s = stats(p);
  return (
    <div class="flex min-w-0 flex-wrap items-center gap-1.5 text-xs">
      <Show when={s.allDisabled}>
        <Badge tone="neutral">
          <Icon name="pause_circle" class="text-[12px]" /> Disabled
        </Badge>
      </Show>
      <Show when={!s.allDisabled && p.no_auth}>
        <Badge tone="green">
          <span class="size-1.5 rounded-full bg-green-500" /> Ready
        </Badge>
      </Show>
      <Show when={!s.allDisabled && !p.no_auth}>
        <Show when={s.connected > 0}>
          <Badge tone="green">
            <span class="size-1.5 rounded-full bg-green-500" /> {s.connected} Connected
          </Badge>
        </Show>
        <Show when={s.error > 0}>
          <Badge tone="red">
            <span class="size-1.5 rounded-full bg-red-500" /> {s.error} Error
          </Badge>
        </Show>
        <Show when={s.connected === 0 && s.error === 0}>
          <span class="text-text-muted">No connections</span>
        </Show>
      </Show>
    </div>
  );
}

function ProviderCard(props: { p: Provider; onToggle: (active: boolean) => void }) {
  const s = () => stats(props.p);
  return (
    <A href={`/providers/${props.p.id}`} class="group min-w-0 no-underline">
      <Card
        padding="xs"
        class={cn(
          "h-full cursor-pointer transition-colors hover:bg-black/[0.01] dark:hover:bg-white/[0.01]",
          s().allDisabled && "opacity-50"
        )}
      >
        <div class="flex min-w-0 items-center justify-between gap-3">
          <div class="flex min-w-0 items-center gap-3">
            <ProviderIcon id={props.p.id} color={props.p.color} textIcon={props.p.text_icon} />
            <div class="min-w-0">
              <h3 class="truncate font-semibold text-text-main">{props.p.display_name}</h3>
              <StatusBadges {...props.p} />
            </div>
          </div>
          <Show when={s().total > 0}>
            <div
              class="shrink-0 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100"
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                props.onToggle(s().allDisabled);
              }}
            >
              <Toggle checked={!s().allDisabled} onChange={() => {}} />
            </div>
          </Show>
        </div>
      </Card>
    </A>
  );
}

export default function Providers() {
  const [data, { refetch }] = createResource(async () => {
    return await api<{ providers: Provider[]; nodes: Node[] }>("/providers");
  });
  const [search, setSearch] = createSignal("");
  const [showNode, setShowNode] = createSignal(false);
  const [nodeForm, setNodeForm] = createSignal({ prefix: "", base_url: "", api_key: "", name: "" });
  const [error, setError] = createSignal("");
  const [testing, setTesting] = createSignal<string | null>(null);

  const match = (p: Provider) =>
    !search().trim() || p.display_name.toLowerCase().includes(search().trim().toLowerCase()) ||
    p.id.includes(search().trim().toLowerCase());

  const byCategory = (cat: string) =>
    (data()?.providers ?? [])
      .filter((p) => p.category === cat && match(p))
      .sort((a, b) => {
        const ca = stats(a).connected > 0 ? 0 : 1;
        const cb = stats(b).connected > 0 ? 0 : 1;
        return ca - cb || a.display_name.localeCompare(b.display_name);
      });

  const apiKeyProviders = () =>
    (data()?.providers ?? [])
      .filter((p) => p.category === "apikey" && !p.no_auth && match(p))
      .sort((a, b) => {
        const ca = stats(a).connected > 0 ? 0 : 1;
        const cb = stats(b).connected > 0 ? 0 : 1;
        return ca - cb || a.display_name.localeCompare(b.display_name);
      });

  const toggleProvider = async (p: Provider, enable: boolean) => {
    await Promise.all(
      p.connections.map((c) =>
        api(`/providers/${c.id}`, { method: "PUT", body: JSON.stringify({ is_active: enable }) })
      )
    );
    refetch();
  };

  const testAll = async (group: string, providers: Provider[]) => {
    setTesting(group);
    try {
      await Promise.all(
        providers.flatMap((p) =>
          p.connections.map((c) => api(`/providers/${c.id}/test`, { method: "POST" }).catch(() => {}))
        )
      );
    } finally {
      setTesting(null);
      refetch();
    }
  };

  const addNode = async () => {
    setError("");
    try {
      await api("/providers/nodes", {
        method: "POST",
        body: JSON.stringify({
          prefix: nodeForm().prefix,
          base_url: nodeForm().base_url,
          api_key: nodeForm().api_key || null,
          name: nodeForm().name || null,
        }),
      });
      setShowNode(false);
      setNodeForm({ prefix: "", base_url: "", api_key: "", name: "" });
      refetch();
    } catch (e) {
      setError(e instanceof Error ? e.message : "failed");
    }
  };

  const TestAllBtn = (props: { group: string; providers: Provider[] }) => (
    <button
      onClick={() => testAll(props.group, props.providers)}
      disabled={testing() !== null}
      class={cn(
        "flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors",
        testing() === props.group
          ? "animate-pulse border-primary/40 bg-primary/20 text-primary"
          : "border-border bg-bg text-text-muted hover:border-primary/40 hover:text-text-main"
      )}
    >
      <Icon name="play_arrow" class={cn("text-[14px]", testing() === props.group && "animate-spin")} />
      {testing() === props.group ? "Testing..." : "Test All"}
    </button>
  );

  const SectionHeader = (props: { title: string; children?: unknown }) => (
    <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <h2 class="flex items-center gap-2 text-lg font-semibold leading-tight sm:text-xl">
        {props.title}
      </h2>
      <div class="flex items-center gap-2">{props.children}</div>
    </div>
  );

  return (
    <div class="flex min-w-0 flex-col gap-6">
      <PageHeader
        title="Providers"
        subtitle="Manage your AI provider connections"
        actions={
          <div class="relative">
            <Icon name="search" class="absolute left-3 top-1/2 -translate-y-1/2 text-[18px] text-text-subtle" />
            <input
              class="h-9 w-64 rounded-[10px] border border-border bg-bg pl-9 pr-3 text-sm text-text-main placeholder:text-text-subtle focus:border-brand-500/50 focus:outline-none focus:shadow-[var(--shadow-focus)]"
              placeholder="Search providers…"
              value={search()}
              onInput={(e) => setSearch(e.currentTarget.value)}
            />
          </div>
        }
      />

      <Show when={error()}>
        <p class="flex items-center gap-1.5 text-sm text-danger">
          <Icon name="error" class="text-[16px]" /> {error()}
        </p>
      </Show>

      {/* Custom Providers */}
      <div class="flex flex-col gap-4">
        <SectionHeader title="Custom Providers (OpenAI Compatible)">
          <Button variant="secondary" size="sm" icon="add" onClick={() => setShowNode(true)}>
            Add OpenAI Compatible
          </Button>
        </SectionHeader>
        <Show
          when={(data()?.nodes.length ?? 0) > 0}
          fallback={
            <div class="flex items-center justify-center gap-2 rounded-xl border border-dashed border-border py-4 text-sm text-text-muted">
              <Icon name="extension" class="text-[18px]" />
              <span>No custom providers — add any OpenAI-compatible endpoint</span>
            </div>
          }
        >
          <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 sm:gap-4 lg:grid-cols-3 xl:grid-cols-4">
            <For each={data()?.nodes}>
              {(n) => (
                <Card padding="xs">
                  <div class="flex min-w-0 items-center justify-between gap-3">
                    <div class="flex min-w-0 items-center gap-3">
                      <div class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-surface-2">
                        <Icon name="lan" class="text-[18px] text-text-muted" />
                      </div>
                      <div class="min-w-0">
                        <h3 class="truncate font-semibold text-text-main">
                          {n.name ?? n.data.prefix}
                        </h3>
                        <p class="truncate font-mono text-xs text-text-muted">
                          {n.data.prefix}/… · {n.data.baseUrl}
                        </p>
                      </div>
                    </div>
                    <Button
                      variant="ghost"
                      size="sm"
                      icon="delete"
                      class="text-danger"
                      onClick={async () => {
                        await api(`/providers/nodes/${n.id}`, { method: "DELETE" });
                        refetch();
                      }}
                    />
                  </div>
                </Card>
              )}
            </For>
          </div>
        </Show>
      </div>

      {/* OAuth Providers */}
      <Show when={byCategory("oauth").length > 0}>
        <div class="flex flex-col gap-4">
          <SectionHeader title="OAuth Providers">
            <TestAllBtn group="oauth" providers={byCategory("oauth")} />
          </SectionHeader>
          <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 sm:gap-4 lg:grid-cols-3 xl:grid-cols-4">
            <For each={byCategory("oauth")}>
              {(p) => <ProviderCard p={p} onToggle={(enable) => toggleProvider(p, enable)} />}
            </For>
          </div>
        </div>
      </Show>

      {/* Free Tier Providers */}
      <Show when={(data()?.providers ?? []).filter((p) => p.no_auth && match(p)).length > 0}>
        <div class="flex flex-col gap-4">
          <SectionHeader title="Free Tier Providers">
            <TestAllBtn group="free" providers={(data()?.providers ?? []).filter((p) => p.no_auth && match(p))} />
          </SectionHeader>
          <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 sm:gap-4 lg:grid-cols-3 xl:grid-cols-4">
            <For each={(data()?.providers ?? []).filter((p) => p.no_auth && match(p))}>
              {(p) => <ProviderCard p={p} onToggle={(enable) => toggleProvider(p, enable)} />}
            </For>
          </div>
        </div>
      </Show>

      {/* API Key Providers */}
      <Show when={apiKeyProviders().length > 0}>
        <div class="flex flex-col gap-4">
          <SectionHeader title="API Key Providers">
            <TestAllBtn group="apikey" providers={apiKeyProviders()} />
          </SectionHeader>
          <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 sm:gap-4 lg:grid-cols-3 xl:grid-cols-4">
            <For each={apiKeyProviders()}>
              {(p) => <ProviderCard p={p} onToggle={(enable) => toggleProvider(p, enable)} />}
            </For>
          </div>
        </div>
      </Show>

      {/* Add node modal */}
      <Modal
        open={showNode()}
        title="Add OpenAI Compatible provider"
        onClose={() => setShowNode(false)}
        footer={
          <>
            <Button variant="secondary" size="sm" onClick={() => setShowNode(false)}>
              Cancel
            </Button>
            <Button size="sm" onClick={addNode}>
              Save
            </Button>
          </>
        }
      >
        <div class="flex flex-col gap-3">
          <label class="text-xs font-medium text-text-muted">
            Prefix (model namespace)
            <div class="mt-1">
              <Input
                placeholder="local"
                value={nodeForm().prefix}
                onInput={(v) => setNodeForm({ ...nodeForm(), prefix: v })}
              />
            </div>
          </label>
          <label class="text-xs font-medium text-text-muted">
            Base URL
            <div class="mt-1">
              <Input
                placeholder="http://localhost:11434/v1"
                value={nodeForm().base_url}
                onInput={(v) => setNodeForm({ ...nodeForm(), base_url: v })}
              />
            </div>
          </label>
          <label class="text-xs font-medium text-text-muted">
            API key (optional)
            <div class="mt-1">
              <Input
                placeholder="sk-…"
                value={nodeForm().api_key}
                onInput={(v) => setNodeForm({ ...nodeForm(), api_key: v })}
              />
            </div>
          </label>
          <label class="text-xs font-medium text-text-muted">
            Name (optional)
            <div class="mt-1">
              <Input
                placeholder="My node"
                value={nodeForm().name}
                onInput={(v) => setNodeForm({ ...nodeForm(), name: v })}
              />
            </div>
          </label>
        </div>
      </Modal>
    </div>
  );
}
