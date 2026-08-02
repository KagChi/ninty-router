import { For, Show, createResource, createSignal } from "solid-js";
import { api } from "~/lib/api";

interface Combo {
  id: string;
  name: string;
  kind: string | null;
  models: string[];
  created_at: string;
}

export default function Combos() {
  const [combos, { refetch }] = createResource(async () => api<Combo[]>("/combos"));
  const [name, setName] = createSignal("");
  const [kind, setKind] = createSignal("general");
  const [modelsText, setModelsText] = createSignal("");
  const [error, setError] = createSignal("");
  const [editing, setEditing] = createSignal<string | null>(null);

  const parseModels = () =>
    modelsText()
      .split("\n")
      .map((m) => m.trim())
      .filter(Boolean);

  const save = async () => {
    setError("");
    const models = parseModels();
    if (!name().trim() || models.length === 0) {
      setError("name and at least one model required");
      return;
    }
    try {
      const body = JSON.stringify({ name: name().trim(), kind: kind(), models });
      const id = editing();
      if (id) {
        await api(`/combos/${id}`, { method: "PUT", body });
      } else {
        await api("/combos", { method: "POST", body });
      }
      setName("");
      setModelsText("");
      setEditing(null);
      refetch();
    } catch (e) {
      setError(e instanceof Error ? e.message : "failed");
    }
  };

  const edit = (c: Combo) => {
    setEditing(c.id);
    setName(c.name);
    setKind(c.kind ?? "general");
    setModelsText(c.models.join("\n"));
  };

  const remove = async (id: string) => {
    await api(`/combos/${id}`, { method: "DELETE" });
    refetch();
  };

  return (
    <div>
      <h1 class="mb-4 text-xl font-semibold">Combos</h1>
      <p class="mb-4 text-sm text-text-muted">
        A combo name can be used as the <code>model</code> in API requests. Models are tried in
        order; fallbackable errors move to the next. One model per line, e.g.{" "}
        <code>openai/gpt-4o</code>.
      </p>

      <div class="mb-6 rounded-lg border border-border bg-surface p-4">
        <div class="mb-2 flex gap-2">
          <input
            class="w-56 rounded border border-border bg-bg px-2 py-1 text-sm"
            placeholder="combo name"
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
          />
          <select
            class="rounded border border-border bg-bg px-2 py-1 text-sm"
            value={kind()}
            onChange={(e) => setKind(e.currentTarget.value)}
          >
            <option value="general">general</option>
            <option value="vision">vision</option>
            <option value="tools">tools</option>
            <option value="free">free</option>
          </select>
        </div>
        <textarea
          class="mb-2 h-32 w-full rounded border border-border bg-bg px-2 py-1 font-mono text-sm"
          placeholder={"openai/gpt-4o\nanthropic/claude-sonnet-4-5-20250929"}
          value={modelsText()}
          onInput={(e) => setModelsText(e.currentTarget.value)}
        />
        <div class="flex items-center gap-3">
          <button
            class="rounded bg-primary px-3 py-1 text-sm font-medium text-black"
            onClick={save}
          >
            {editing() ? "Update" : "Create"}
          </button>
          <Show when={editing()}>
            <button
              class="text-sm text-text-muted"
              onClick={() => {
                setEditing(null);
                setName("");
                setModelsText("");
              }}
            >
              cancel
            </button>
          </Show>
          <Show when={error()}>
            <span class="text-sm text-red-400">{error()}</span>
          </Show>
        </div>
      </div>

      <Show when={combos()} fallback={<p class="text-sm text-text-muted">Loading…</p>}>
        <For each={combos()}>
          {(c) => (
            <div class="mb-2 flex items-start justify-between rounded-lg border border-border bg-surface p-4">
              <div>
                <div class="font-medium">
                  {c.name}
                  <span class="ml-2 rounded bg-bg px-1.5 py-0.5 text-xs text-text-muted">
                    {c.kind ?? "general"}
                  </span>
                </div>
                <ol class="mt-1 list-decimal pl-5 font-mono text-sm text-text-muted">
                  <For each={c.models}>{(m) => <li>{m}</li>}</For>
                </ol>
              </div>
              <div class="flex gap-3 text-sm">
                <button class="text-primary" onClick={() => edit(c)}>
                  edit
                </button>
                <button class="text-red-400" onClick={() => remove(c.id)}>
                  delete
                </button>
              </div>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}
