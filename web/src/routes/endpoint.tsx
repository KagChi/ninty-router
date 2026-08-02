import { For, Show, createResource, createSignal } from "solid-js";
import { api, type ApiKey } from "~/lib/api";
import { Button, Card, CardSection, Icon, Input, PageHeader, TableSkeleton } from "~/components/ui";

export default function Endpoint() {
  const [keys, { refetch }] = createResource(async () => {
    const res = await api<{ keys: ApiKey[] }>("/keys");
    return res.keys;
  });
  const [showCreate, setShowCreate] = createSignal(false);
  const [name, setName] = createSignal("");
  const [rpm, setRpm] = createSignal("");
  const [copied, setCopied] = createSignal("");
  const [error, setError] = createSignal("");

  const baseUrl = () => `${window.location.protocol}//${window.location.host}/v1`;

  const copy = async (text: string, tag: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(tag);
    setTimeout(() => setCopied(""), 1500);
  };

  const createKey = async (e: Event) => {
    e.preventDefault();
    setError("");
    try {
      await api("/keys", {
        method: "POST",
        body: JSON.stringify({
          name: name() || null,
          rpm_limit: rpm() ? Number(rpm()) : null,
        }),
      });
      setName("");
      setRpm("");
      setShowCreate(false);
      refetch();
    } catch (err) {
      setError(err instanceof Error ? err.message : "failed");
    }
  };

  const remove = async (id: string) => {
    await api(`/keys/${id}`, { method: "DELETE" });
    refetch();
  };

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

      <Card
        title="API Keys"
        icon="key"
        action={
          <Button size="sm" icon="add" onClick={() => setShowCreate(true)}>
            New key
          </Button>
        }
      >
        <Show when={showCreate()}>
          <CardSection class="mb-4">
            <form onSubmit={createKey} class="flex items-end gap-2">
              <label class="flex-1 text-xs font-medium text-text-muted">
                Name
                <div class="mt-1">
                  <Input value={name()} onInput={setName} placeholder="my-key" />
                </div>
              </label>
              <label class="w-32 text-xs font-medium text-text-muted">
                RPM limit
                <div class="mt-1">
                  <Input type="number" value={rpm()} onInput={setRpm} placeholder="∞" />
                </div>
              </label>
              <Button type="submit" size="sm">
                Create
              </Button>
              <Button variant="secondary" size="sm" onClick={() => setShowCreate(false)}>
                Cancel
              </Button>
            </form>
          </CardSection>
        </Show>

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
            <div class="overflow-x-auto">
              <table class="w-full text-sm">
              <thead>
                <tr class="border-b border-border-subtle text-left text-xs font-semibold text-text-muted">
                  <th class="py-2 pr-4">Name</th>
                  <th class="py-2 pr-4">Key</th>
                  <th class="py-2 pr-4">RPM</th>
                  <th class="py-2 pr-4">Created</th>
                  <th class="py-2" />
                </tr>
              </thead>
              <tbody>
                <For each={keys()}>
                  {(k) => (
                    <tr class="border-b border-border-subtle last:border-b-0">
                      <td class="py-2.5 pr-4 font-medium">{k.name ?? "—"}</td>
                      <td class="py-2.5 pr-4">
                        <button
                          class="font-mono text-xs text-brand-500 hover:underline"
                          title="click to copy"
                          onClick={() => copy(k.key, k.id)}
                        >
                          {copied() === k.id ? "✓ copied" : `${k.key.slice(0, 12)}…`}
                        </button>
                      </td>
                      <td class="py-2.5 pr-4">{k.rpm_limit ?? "∞"}</td>
                      <td class="py-2.5 pr-4 text-text-muted">
                        {new Date(k.created_at).toLocaleDateString()}
                      </td>
                      <td class="py-2.5 text-right">
                        <Button variant="ghost" size="sm" icon="delete" onClick={() => remove(k.id)} />
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>

            </div>
          </Show>
        </Show>
      </Card>
    </div>
  );
}
