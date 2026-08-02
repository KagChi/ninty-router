import { For, Show, createResource, createSignal } from "solid-js";
import { api } from "~/lib/api";
import { Badge, Button, Card, Icon, Input, PageHeader, Select, Textarea, CardSkeleton } from "~/components/ui";

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
      <PageHeader
        title="Combos"
        subtitle="A combo name works as the model in API requests — models are tried in order, fallbackable errors move to the next"
      />

      <Card title={editing() ? `Edit ${editing()}` : "New combo"} icon="layers" class="mb-6">
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
        <Textarea
          rows={6}
          class="mb-3 font-mono"
          placeholder={"openrouter/gpt-4o\nglm/glm-4.6 — one model per line"}
          value={modelsText()}
          onInput={setModelsText}
        />
        <div class="flex items-center gap-2">
          <Button size="sm" onClick={save}>{editing() ? "Update" : "Create"}</Button>
          <Show when={editing()}>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                setEditing(null);
                setName("");
                setModelsText("");
              }}
            >
              cancel
            </Button>
          </Show>
          <Show when={error()}>
            <span class="flex items-center gap-1.5 text-sm text-danger">
              <Icon name="error" class="text-[16px]" /> {error()}
            </span>
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
    </div>
  );
}
