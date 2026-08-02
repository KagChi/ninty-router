import { For, Show, createResource, createSignal } from "solid-js";
import { api, type ApiKey } from "~/lib/api";

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
    <div>
      <h1 class="mb-6 text-xl font-semibold">Endpoint & API Keys</h1>

      <section class="mb-6 rounded-lg border border-border bg-surface p-4">
        <div class="mb-1 text-sm text-text-muted">Base URL (use in your CLI tools)</div>
        <div class="flex items-center gap-2">
          <code class="rounded bg-bg px-3 py-2 text-sm text-primary">{baseUrl()}</code>
          <button
            class="rounded-md border border-border px-3 py-1.5 text-sm text-text-muted hover:text-text"
            onClick={() => copy(baseUrl(), "url")}
          >
            {copied() === "url" ? "copied" : "copy"}
          </button>
        </div>
        <p class="mt-2 text-xs text-text-muted">
          Auth: <code>Authorization: Bearer &lt;key&gt;</code> — point Claude Code, Cursor,
          Cline, Codex… at this URL.
        </p>
      </section>

      <section class="rounded-lg border border-border bg-surface p-4">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="font-medium">API Keys</h2>
          <button
            class="rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-white"
            onClick={() => setShowCreate(true)}
          >
            + New key
          </button>
        </div>

        <Show when={showCreate()}>
          <form
            onSubmit={createKey}
            class="mb-4 flex items-end gap-2 rounded-md border border-border bg-bg p-3"
          >
            <label class="flex-1 text-xs text-text-muted">
              Name
              <input
                class="mt-1 w-full rounded border border-border bg-surface px-2 py-1.5 text-sm text-text"
                value={name()}
                onInput={(e) => setName(e.currentTarget.value)}
                placeholder="my-key"
              />
            </label>
            <label class="w-32 text-xs text-text-muted">
              RPM limit
              <input
                type="number"
                class="mt-1 w-full rounded border border-border bg-surface px-2 py-1.5 text-sm text-text"
                value={rpm()}
                onInput={(e) => setRpm(e.currentTarget.value)}
                placeholder="∞"
              />
            </label>
            <button type="submit" class="rounded-md bg-primary px-3 py-1.5 text-sm text-white">
              Create
            </button>
            <button
              type="button"
              class="rounded-md border border-border px-3 py-1.5 text-sm text-text-muted"
              onClick={() => setShowCreate(false)}
            >
              Cancel
            </button>
          </form>
        </Show>

        {error() && <p class="mb-2 text-sm text-danger">{error()}</p>}

        <Show when={keys()} fallback={<p class="text-sm text-text-muted">loading…</p>}>
          <Show
            when={(keys()?.length ?? 0) > 0}
            fallback={<p class="text-sm text-text-muted">No keys yet.</p>}
          >
            <table class="w-full text-sm">
              <thead>
                <tr class="border-b border-border text-left text-xs text-text-muted">
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
                    <tr class="border-b border-border/50">
                      <td class="py-2 pr-4">{k.name ?? "—"}</td>
                      <td class="py-2 pr-4">
                        <button
                          class="text-primary hover:underline"
                          title="click to copy"
                          onClick={() => copy(k.key, k.id)}
                        >
                          {copied() === k.id ? "copied" : `${k.key.slice(0, 12)}…`}
                        </button>
                      </td>
                      <td class="py-2 pr-4">{k.rpm_limit ?? "∞"}</td>
                      <td class="py-2 pr-4 text-text-muted">
                        {new Date(k.created_at).toLocaleDateString()}
                      </td>
                      <td class="py-2 text-right">
                        <button
                          class="text-danger hover:underline"
                          onClick={() => remove(k.id)}
                        >
                          delete
                        </button>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </Show>
        </Show>
      </section>
    </div>
  );
}
