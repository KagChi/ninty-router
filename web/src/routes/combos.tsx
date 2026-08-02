import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { api } from "~/lib/api";
import { Badge, Button, Card, Icon, Input, PageHeader, Select, Modal, CardSkeleton } from "~/components/ui";
import { toast } from "~/lib/toast";
import type { Provider } from "~/components/provider-bits";

interface Combo {
  id: string;
  name: string;
  kind: string | null;
  models: string[];
  created_at: string;
}

export default function Combos() {
  const [combos, { refetch }] = createResource(async () => api<Combo[]>("/combos"));
  const [providers] = createResource(async () => {
    const res = await api<{ providers: Provider[] }>("/providers");
    return res.providers;
  });
  const [name, setName] = createSignal("");
  const [kind, setKind] = createSignal("general");
  const [models, setModels] = createSignal<string[]>([]);
  const [pickerOpen, setPickerOpen] = createSignal(false);
  const [pickerSearch, setPickerSearch] = createSignal("");
  const [editing, setEditing] = createSignal<string | null>(null);

  const allModels = createMemo(() => {
    const out: { spec: string; provider: string }[] = [];
    for (const p of providers() ?? []) {
      for (const m of p.models ?? []) {
        if (!m.disabled) out.push({ spec: `${p.alias}/${m.id}`, provider: p.display_name });
      }
    }
    return out;
  });

  const filteredModels = createMemo(() => {
    const q = pickerSearch().trim().toLowerCase();
    if (!q) return allModels();
    return allModels().filter(
      (m) => m.spec.toLowerCase().includes(q) || m.provider.toLowerCase().includes(q)
    );
  });

  const addModel = (spec: string) => {
    if (!models().includes(spec)) setModels([...models(), spec]);
    setPickerOpen(false);
    setPickerSearch("");
  };

  const save = async () => {
    if (!name().trim() || models().length === 0) {
      toast.error("name and at least one model required");
      return;
    }
    try {
      const body = JSON.stringify({ name: name().trim(), kind: kind(), models: models() });
      const id = editing();
      if (id) {
        await api(`/combos/${id}`, { method: "PUT", body });
        toast.success(`combo '${name()}' updated`);
      } else {
        await api("/combos", { method: "POST", body });
        toast.success(`combo '${name()}' created`);
      }
      setName("");
      setModels([]);
      setEditing(null);
      refetch();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "failed");
    }
  };

  const edit = (c: Combo) => {
    setEditing(c.id);
    setName(c.name);
    setKind(c.kind ?? "general");
    setModels(c.models);
  };

  const remove = async (id: string) => {
    await api(`/combos/${id}`, { method: "DELETE" });
    toast.success("combo deleted");
    refetch();
  };

  return (
    <div class="flex min-w-0 flex-col gap-6">
      <div>
        <PageHeader
          title="Combos"
          subtitle="Group models under one name, then pick a strategy per combo:"
        />
        <ul class="mt-2 flex flex-col gap-1 text-sm text-text-muted">
          <li><span class="font-medium text-text-main">Fallback</span> — tries models in order (next on failure)</li>
          <li><span class="font-medium text-text-main">Round Robin</span> — rotates models across requests to spread load</li>
          <li><span class="font-medium text-text-main">Fusion</span> — queries all models in parallel, then a judge synthesizes one answer. Best quality, but costs the most (N+1 calls)</li>
          <li><span class="font-medium text-text-main">Capacity auto-switch</span> — sends image/PDF/audio requests to a model that supports them first</li>
        </ul>
      </div>

      <Card title={editing() ? `Edit ${name()}` : "New combo"} icon="layers">
        <div class="mb-3 flex gap-2">
          <div class="w-56">
            <Input placeholder="combo name" value={name()} onInput={setName} />
          </div>
          <Select value={kind()} onChange={setKind}>
            <option value="general">general</option>
            <option value="vision">vision</option>
            <option value="tools">tools</option>
            <option value="free">free</option>
          </Select>
        </div>

        {/* selected model chips (ordered = fallback order) */}
        <div class="mb-3 flex flex-wrap gap-2">
          <For each={models()}>
            {(m, i) => (
              <span class="flex items-center gap-1.5 rounded-[10px] border border-border bg-bg px-2.5 py-1.5">
                <span class="text-[11px] text-text-subtle">{i() + 1}.</span>
                <code class="font-mono text-xs text-text-main">{m}</code>
                <button
                  class="text-text-muted hover:text-danger"
                  title="remove"
                  onClick={() => setModels(models().filter((x) => x !== m))}
                >
                  <Icon name="close" class="text-[13px]" />
                </button>
              </span>
            )}
          </For>
          <button
            class="flex items-center gap-1 rounded-[10px] border border-dashed border-border px-2.5 py-1.5 text-xs text-text-muted transition-colors hover:border-brand-500/50 hover:text-brand-500"
            onClick={() => setPickerOpen(true)}
          >
            <Icon name="add" class="text-[14px]" /> Add model
          </button>
        </div>

        <div class="flex items-center gap-2">
          <Button size="sm" onClick={save}>{editing() ? "Update" : "Create"}</Button>
          <Show when={editing()}>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                setEditing(null);
                setName("");
                setModels([]);
              }}
            >
              cancel
            </Button>
          </Show>
        </div>
      </Card>

      <Show when={combos()} fallback={<><CardSkeleton /><CardSkeleton /></>}>
        <For each={combos()}>
          {(c) => (
            <Card padding="sm" class="mb-2 flex items-start justify-between">
              <div>
                <div class="font-semibold text-text-main">
                  {c.name}
                  <Badge tone="brand">{c.kind ?? "general"}</Badge>
                </div>
                <ol class="mt-1 list-decimal pl-5 font-mono text-sm text-text-muted">
                  <For each={c.models}>{(m) => <li>{m}</li>}</For>
                </ol>
              </div>
              <div class="flex gap-1">
                <Button variant="ghost" size="sm" icon="edit" onClick={() => edit(c)} />
                <Button variant="ghost" size="sm" icon="delete" class="text-danger" onClick={() => remove(c.id)} />
              </div>
            </Card>
          )}
        </For>
      </Show>

      {/* searchable model picker modal */}
      <Modal
        open={pickerOpen()}
        title="Add model"
        onClose={() => { setPickerOpen(false); setPickerSearch(""); }}
      >
        <Input
          placeholder="search models…"
          value={pickerSearch()}
          onInput={setPickerSearch}
        />
        <div class="mt-2 flex max-h-72 flex-col gap-0.5 overflow-y-auto">
          <For each={filteredModels()} fallback={<p class="p-2 text-xs text-text-subtle">no match</p>}>
            {(m) => (
              <button
                class="flex items-center gap-2 rounded-md px-2 py-1.5 text-left hover:bg-surface-2 disabled:opacity-40"
                disabled={models().includes(m.spec)}
                onClick={() => addModel(m.spec)}
              >
                <code class="font-mono text-xs text-text-main">{m.spec}</code>
                <span class="text-xs text-text-subtle">{m.provider}</span>
                <Show when={models().includes(m.spec)}>
                  <Icon name="check" class="ml-auto text-[14px] text-green-500" />
                </Show>
              </button>
            )}
          </For>
        </div>
      </Modal>
    </div>
  );
}
