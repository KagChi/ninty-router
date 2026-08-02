import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { api, type ApiKey } from "~/lib/api";
import { Badge, Button, Card, Icon, Input, Modal, PageHeader, Select, TableSkeleton, Toggle } from "~/components/ui";
import type { Provider } from "~/components/provider-bits";

interface KeyForm {
  name: string;
  token_limit: string;
  limit_window: string;
  rpm: string;
  allowed_models: string[];
}

const EMPTY_FORM: KeyForm = { name: "", token_limit: "", limit_window: "total", rpm: "", allowed_models: [] };

export default function Endpoint() {
  const [keys, { refetch }] = createResource(async () => {
    const res = await api<{ keys: ApiKey[] }>("/keys");
    return res.keys;
  });
  const [aliases, { refetch: refetchAliases }] = createResource(async () => {
    const res = await api<{ aliases: Record<string, string> }>("/models/alias");
    return res.aliases;
  });
  const [providers] = createResource(async () => {
    const res = await api<{ providers: Provider[] }>("/providers");
    return res.providers;
  });

  const [showCreate, setShowCreate] = createSignal(false);
  const [editing, setEditing] = createSignal<ApiKey | null>(null);
  const [form, setForm] = createSignal<KeyForm>(EMPTY_FORM);
  const [createdKey, setCreatedKey] = createSignal<ApiKey | null>(null);
  const [copied, setCopied] = createSignal("");
  const [error, setError] = createSignal("");
  const [modelPicker, setModelPicker] = createSignal("");
  const [aliasError, setAliasError] = createSignal("");

  const baseUrl = () => `${window.location.protocol}//${window.location.host}/v1`;

  /** All selectable model specs (`alias/model`) across providers. */
  const allModels = createMemo(() => {
    const out: { spec: string; provider: string; id: string }[] = [];
    for (const p of providers() ?? []) {
      for (const m of p.models ?? []) {
        if (!m.disabled) out.push({ spec: `${p.alias}/${m.id}`, provider: p.display_name, id: m.id });
      }
    }
    return out;
  });

  const filteredModels = createMemo(() => {
    const q = modelPicker().trim().toLowerCase();
    if (!q) return allModels();
    return allModels().filter((m) => m.spec.toLowerCase().includes(q) || m.provider.toLowerCase().includes(q));
  });

  const copy = async (text: string, tag: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(tag);
    setTimeout(() => setCopied(""), 1500);
  };

  const payload = () => ({
    name: form().name || null,
    token_limit: form().token_limit ? Number(form().token_limit) : null,
    limit_window: form().limit_window || null,
    rpm_limit: form().rpm ? Number(form().rpm) : null,
    allowed_models: form().allowed_models.length > 0 ? form().allowed_models : null,
  });

  const createKey = async () => {
    setError("");
    try {
      const res = await api<{ key: ApiKey }>("/keys", { method: "POST", body: JSON.stringify(payload()) });
      setForm(EMPTY_FORM);
      setShowCreate(false);
      setCreatedKey(res.key);
      refetch();
    } catch (err) {
      setError(err instanceof Error ? err.message : "failed");
    }
  };

  const saveEdit = async () => {
    const k = editing();
    if (!k) return;
    setError("");
    try {
      await api(`/keys/${k.id}`, { method: "PUT", body: JSON.stringify(payload()) });
      setEditing(null);
      refetch();
    } catch (err) {
      setError(err instanceof Error ? err.message : "failed");
    }
  };

  const openEdit = (k: ApiKey) => {
    setForm({
      name: k.name ?? "",
      token_limit: k.token_limit != null ? String(k.token_limit) : "",
      limit_window: k.limit_window ?? "total",
      rpm: k.rpm_limit != null ? String(k.rpm_limit) : "",
      allowed_models: k.allowed_models ?? [],
    });
    setEditing(k);
  };

  const toggleActive = async (k: ApiKey, active: boolean) => {
    await api(`/keys/${k.id}`, { method: "PUT", body: JSON.stringify({ is_active: active }) });
    refetch();
  };

  const resetUsage = async (k: ApiKey) => {
    await api(`/keys/${k.id}/reset`, { method: "POST" });
    refetch();
  };

  const remove = async (id: string) => {
    await api(`/keys/${id}`, { method: "DELETE" });
    refetch();
  };

  const deleteAlias = async (alias: string) => {
    setAliasError("");
    try {
      await api(`/models/alias?alias=${encodeURIComponent(alias)}`, { method: "DELETE" });
      refetchAliases();
    } catch (e) {
      setAliasError(e instanceof Error ? e.message : "failed");
    }
  };

  const deleteAllAliases = async () => {
    for (const a of Object.keys(aliases() ?? {})) {
      await api(`/models/alias?alias=${encodeURIComponent(a)}`, { method: "DELETE" });
    }
    refetchAliases();
  };

  const toggleModel = (spec: string) => {
    setForm((f) => ({
      ...f,
      allowed_models: f.allowed_models.includes(spec)
        ? f.allowed_models.filter((m) => m !== spec)
        : [...f.allowed_models, spec],
    }));
  };

  /** Create/Edit shared form body. */
  const KeyFormFields = () => (
    <div class="flex flex-col gap-3">
      <label class="text-xs font-medium text-text-muted">
        Name
        <div class="mt-1">
          <Input value={form().name} onInput={(v) => setForm({ ...form(), name: v })} placeholder="my-key" />
        </div>
      </label>
      <div class="grid grid-cols-2 gap-3">
        <label class="text-xs font-medium text-text-muted">
          Token limit (0 = unlimited)
          <div class="mt-1">
            <Input
              type="number"
              value={form().token_limit}
              onInput={(v) => setForm({ ...form(), token_limit: v })}
              placeholder="∞"
            />
          </div>
        </label>
        <label class="text-xs font-medium text-text-muted">
          Window
          <div class="mt-1">
            <Select
              class="w-full"
              value={form().limit_window}
              onChange={(v) => setForm({ ...form(), limit_window: v })}
            >
              <option value="total">Total</option>
              <option value="daily">Daily</option>
              <option value="monthly">Monthly</option>
            </Select>
          </div>
        </label>
      </div>
      <label class="text-xs font-medium text-text-muted">
        RPM limit (0 = unlimited)
        <div class="mt-1">
          <Input
            type="number"
            value={form().rpm}
            onInput={(v) => setForm({ ...form(), rpm: v })}
            placeholder="∞"
          />
        </div>
      </label>
      <div class="text-xs font-medium text-text-muted">
        Allowed models ({form().allowed_models.length === 0 ? "all" : form().allowed_models.length})
        <div class="mt-1">
          <Input placeholder="search models…" value={modelPicker()} onInput={setModelPicker} />
        </div>
        <div class="mt-2 flex max-h-40 flex-col gap-0.5 overflow-y-auto rounded-[10px] border border-border bg-bg p-1.5">
          <For each={filteredModels()} fallback={<p class="p-2 text-xs text-text-subtle">no match</p>}>
            {(m) => (
              <label class="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1 text-xs hover:bg-surface-2">
                <input
                  type="checkbox"
                  class="accent-brand-500"
                  checked={form().allowed_models.includes(m.spec)}
                  onChange={() => toggleModel(m.spec)}
                />
                <code class="font-mono text-text-main">{m.spec}</code>
                <span class="text-text-subtle">{m.provider}</span>
              </label>
            )}
          </For>
        </div>
      </div>
    </div>
  );

  return (
    <div class="flex flex-col gap-6">
      <PageHeader
        title="Endpoint & API Keys"
        subtitle="Point your CLI tools at this router — one URL for every provider"
      />

      <Card title="Base URL" icon="link">
        <div class="flex items-center gap-2">
          <code class="rounded-[10px] bg-bg px-3 py-2 font-mono text-sm text-brand-500">
            {baseUrl()}
          </code>
          <Button
            variant="secondary"
            size="sm"
            icon={copied() === "url" ? "check" : "content_copy"}
            onClick={() => copy(baseUrl(), "url")}
          >
            {copied() === "url" ? "copied" : "copy"}
          </Button>
        </div>
        <p class="mt-3 text-xs text-text-muted">
          Auth: <code class="font-mono">Authorization: Bearer &lt;key&gt;</code> — Claude Code,
          Cursor, Cline, Codex… all point here. Anthropic-native tools use{" "}
          <code class="font-mono">/v1/messages</code>, Gemini tools{" "}
          <code class="font-mono">/v1beta</code>.
        </p>
      </Card>

      {/* Model Aliases card (9router endpoint page) */}
      <Card
        title={`Model Aliases (${Object.keys(aliases() ?? {}).length})`}
        icon="label"
        action={
          <Show when={Object.keys(aliases() ?? {}).length > 0}>
            <Button variant="ghost" size="sm" class="text-danger" onClick={deleteAllAliases}>
              Delete all
            </Button>
          </Show>
        }
      >
        <Show when={aliasError()}>
          <p class="mb-3 flex items-center gap-1.5 text-sm text-danger">
            <Icon name="error" class="text-[16px]" /> {aliasError()}
          </p>
        </Show>
        <Show
          when={Object.keys(aliases() ?? {}).length > 0}
          fallback={
            <p class="py-4 text-center text-sm text-text-muted">
              No aliases — set one from a provider's model chip (copy icon).
            </p>
          }
        >
          <div class="flex flex-col gap-1.5">
            <For each={Object.entries(aliases() ?? {})}>
              {([alias, target]) => (
                <div class="flex items-center gap-3 rounded-[10px] border border-border-subtle px-3 py-2">
                  <code class="font-mono text-sm font-semibold text-brand-500">{alias}</code>
                  <Icon name="arrow_forward" class="text-[14px] text-text-subtle" />
                  <code class="min-w-0 flex-1 truncate font-mono text-xs text-text-muted">{target}</code>
                  <button
                    class="rounded p-1 text-text-muted hover:bg-surface-2 hover:text-danger"
                    title="delete alias"
                    onClick={() => deleteAlias(alias)}
                  >
                    <Icon name="close" class="text-[14px]" />
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Card>

      <Card
        title="API Keys"
        icon="key"
        action={
          <Button size="sm" icon="add" onClick={() => { setForm(EMPTY_FORM); setShowCreate(true); }}>
            New key
          </Button>
        }
      >
        <Show when={error()}>
          <p class="mb-3 flex items-center gap-1.5 text-sm text-danger">
            <Icon name="error" class="text-[16px]" /> {error()}
          </p>
        </Show>

        <Show when={keys()} fallback={<TableSkeleton rows={3} />}>
          <Show
            when={(keys()?.length ?? 0) > 0}
            fallback={
              <p class="py-4 text-center text-sm text-text-muted">
                No keys yet — any bearer token works while “Require API key” is off.
              </p>
            }
          >
            <div class="flex flex-col gap-2">
              <For each={keys()}>
                {(k) => {
                  const atCap = () => k.token_limit != null && (k.used ?? 0) >= k.token_limit;
                  const pct = () =>
                    k.token_limit ? Math.min(100, Math.round(((k.used ?? 0) / k.token_limit) * 100)) : 0;
                  return (
                    <div class="flex flex-col gap-2 rounded-[10px] border border-border-subtle px-3 py-2.5 sm:flex-row sm:items-center sm:gap-4">
                      <div class="min-w-0 sm:w-44">
                        <div class="flex items-center gap-2">
                          <span class="truncate font-medium">{k.name ?? "—"}</span>
                          <Show when={!k.is_active}>
                            <Badge tone="neutral">paused</Badge>
                          </Show>
                        </div>
                        <button
                          class="font-mono text-xs text-brand-500 hover:underline"
                          title="click to copy"
                          onClick={() => copy(k.key, k.id)}
                        >
                          {copied() === k.id ? "✓ copied" : `${k.key.slice(0, 12)}…`}
                        </button>
                      </div>

                      {/* usage progress bar */}
                      <div class="min-w-0 flex-1">
                        <Show
                          when={k.token_limit != null}
                          fallback={<p class="text-xs text-text-muted">unlimited · used {(k.used ?? 0).toLocaleString()}</p>}
                        >
                          <div class="flex items-center gap-2 text-xs text-text-muted">
                            <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-2">
                              <div
                                class={`h-full rounded-full ${atCap() ? "bg-red-500" : "bg-brand-500"}`}
                                style={{ width: `${pct()}%` }}
                              />
                            </div>
                            <span class={atCap() ? "font-medium text-red-500" : ""}>
                              {(k.used ?? 0).toLocaleString()} / {k.token_limit!.toLocaleString()}
                            </span>
                            <span class="text-text-subtle">{k.limit_window ?? "total"}</span>
                          </div>
                        </Show>
                        <Show when={(k.allowed_models?.length ?? 0) > 0}>
                          <p class="mt-0.5 truncate text-[11px] text-text-subtle">
                            models: {k.allowed_models.join(", ")}
                          </p>
                        </Show>
                      </div>

                      <div class="flex items-center gap-1.5">
                        <span class="text-xs text-text-subtle">RPM {k.rpm_limit ?? "∞"}</span>
                        <Toggle checked={k.is_active} onChange={() => toggleActive(k, !k.is_active)} />
                        <button
                          class="rounded p-1.5 text-text-muted hover:bg-surface-2 hover:text-text-main"
                          title="reset usage"
                          onClick={() => resetUsage(k)}
                        >
                          <Icon name="restart_alt" class="text-[16px]" />
                        </button>
                        <button
                          class="rounded p-1.5 text-text-muted hover:bg-surface-2 hover:text-text-main"
                          title="edit"
                          onClick={() => openEdit(k)}
                        >
                          <Icon name="edit" class="text-[16px]" />
                        </button>
                        <button
                          class="rounded p-1.5 text-text-muted hover:bg-surface-2 hover:text-danger"
                          title="delete"
                          onClick={() => remove(k.id)}
                        >
                          <Icon name="delete" class="text-[16px]" />
                        </button>
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
          </Show>
        </Show>
      </Card>

      {/* Create modal */}
      <Modal
        open={showCreate()}
        title="Create API key"
        onClose={() => setShowCreate(false)}
        footer={
          <>
            <Button variant="secondary" size="sm" onClick={() => setShowCreate(false)}>
              Cancel
            </Button>
            <Button size="sm" onClick={createKey}>
              Create
            </Button>
          </>
        }
      >
        <KeyFormFields />
      </Modal>

      {/* Edit modal */}
      <Modal
        open={editing() !== null}
        title={`Edit key ${editing()?.name ?? ""}`}
        onClose={() => setEditing(null)}
        footer={
          <>
            <Button variant="secondary" size="sm" onClick={() => setEditing(null)}>
              Cancel
            </Button>
            <Button size="sm" onClick={saveEdit}>
              Save
            </Button>
          </>
        }
      >
        <KeyFormFields />
      </Modal>

      {/* API Key Created dialog (save-now warning, 9router parity) */}
      <Modal
        open={createdKey() !== null}
        title="API Key Created"
        onClose={() => setCreatedKey(null)}
        footer={
          <Button
            size="sm"
            icon={copied() === "newkey" ? "check" : "content_copy"}
            onClick={() => copy(createdKey()!.key, "newkey")}
          >
            {copied() === "newkey" ? "Copied" : "Copy key"}
          </Button>
        }
      >
        <p class="text-sm text-text-muted">
          Copy this key now — treat it like a password and store it somewhere safe.
        </p>
        <code class="mt-3 block break-all rounded-[10px] bg-bg px-3 py-2 font-mono text-sm text-brand-500">
          {createdKey()?.key}
        </code>
      </Modal>
    </div>
  );
}
