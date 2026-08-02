import { For, Show, createResource, createSignal } from "solid-js";
import { api } from "~/lib/api";

interface Connection {
  id: string;
  provider: string;
  auth_type: string;
  name: string | null;
  priority: number;
  is_active: boolean;
  data: Record<string, unknown> & { apiKey?: string; testStatus?: string; lastError?: string };
}

interface Provider {
  id: string;
  alias: string;
  category: string;
  display_name: string;
  notice_url: string | null;
  models: { id: string; name: string }[];
  connections: Connection[];
}

interface Node {
  id: string;
  name: string | null;
  data: { prefix?: string; baseUrl?: string; apiKey?: string };
}

export default function Providers() {
  const [data, { refetch }] = createResource(async () => {
    return await api<{ providers: Provider[]; nodes: Node[] }>("/providers");
  });
  const [addingFor, setAddingFor] = createSignal<string | null>(null);
  const [keyInput, setKeyInput] = createSignal("");
  const [nameInput, setNameInput] = createSignal("");
  const [error, setError] = createSignal("");
  const [testResult, setTestResult] = createSignal<Record<string, string>>({});
  const [showNode, setShowNode] = createSignal(false);
  const [oauthFor, setOauthFor] = createSignal<string | null>(null);
  const [oauthInfo, setOauthInfo] = createSignal<{ url?: string; user_code?: string; verification_uri?: string; state?: string } | null>(null);
  const [oauthCode, setOauthCode] = createSignal("");
  const [oauthStatus, setOauthStatus] = createSignal("");

  const startOauth = async (provider: string) => {
    setOauthFor(provider);
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
          setOauthStatus("connected");
          setOauthFor(null);
          setOauthInfo(null);
          refetch();
        } else if (d.status !== "pending" && d.status !== "authorization_pending" && d.status !== "slow_down") {
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
    const provider = oauthFor();
    if (!provider || !oauthCode().trim()) return;
    setOauthStatus("");
    try {
      await api(`/oauth/${provider}/exchange`, {
        method: "POST",
        body: JSON.stringify({ code: oauthCode().trim(), state: oauthInfo()?.state }),
      });
      setOauthStatus("connected");
      setOauthFor(null);
      setOauthInfo(null);
      setOauthCode("");
      refetch();
    } catch (e) {
      setOauthStatus(e instanceof Error ? e.message : "exchange failed");
    }
  };
  const [nodeForm, setNodeForm] = createSignal({ prefix: "", base_url: "", api_key: "", name: "" });

  const addKey = async (provider: string) => {
    setError("");
    try {
      await api("/providers", {
        method: "POST",
        body: JSON.stringify({
          provider,
          api_key: keyInput(),
          name: nameInput() || null,
        }),
      });
      setKeyInput("");
      setNameInput("");
      setAddingFor(null);
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
    await api(`/providers/${id}`, {
      method: "PUT",
      body: JSON.stringify({ is_active: active }),
    });
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

  return (
    <div>
      <h1 class="mb-6 text-xl font-semibold">Providers</h1>
      {error() && <p class="mb-3 text-sm text-danger">{error()}</p>}

      <Show when={oauthFor()}>
        <div class="mb-4 rounded-lg border border-primary/40 bg-surface p-4">
          <div class="mb-2 flex items-center justify-between">
            <span class="font-medium">Connect {oauthFor()}</span>
            <button class="text-sm text-text-muted" onClick={() => { setOauthFor(null); setOauthInfo(null); }}>
              close
            </button>
          </div>
          <Show when={oauthInfo()?.url}>
            <a href={oauthInfo()!.url} target="_blank" rel="noreferrer" class="text-sm text-primary hover:underline">
              open authorize page ↗
            </a>
            <p class="mt-1 text-xs text-text-muted">Sign in, then paste the code below.</p>
            <div class="mt-2 flex gap-2">
              <input
                class="flex-1 rounded border border-border bg-bg px-2 py-1 font-mono text-sm"
                placeholder="paste code (code#state)"
                value={oauthCode()}
                onInput={(e) => setOauthCode(e.currentTarget.value)}
              />
              <button class="rounded bg-primary px-3 py-1 text-sm font-medium text-black" onClick={submitOauthCode}>
                exchange
              </button>
            </div>
          </Show>
          <Show when={oauthInfo()?.user_code}>
            <p class="text-sm">
              Go to{" "}
              <a href={oauthInfo()!.verification_uri} target="_blank" rel="noreferrer" class="text-primary hover:underline">
                {oauthInfo()!.verification_uri}
              </a>{" "}
              and enter code:
            </p>
            <p class="my-2 font-mono text-lg tracking-widest">{oauthInfo()!.user_code}</p>
            <p class="text-xs text-text-muted">waiting for confirmation…</p>
          </Show>
          <Show when={!oauthInfo()}>
            <p class="text-sm text-text-muted">starting…</p>
          </Show>
          <Show when={oauthStatus()}>
            <p class="mt-2 text-sm text-danger">{oauthStatus()}</p>
          </Show>
        </div>
      </Show>

      <div class="mb-8 flex flex-col gap-3">
        <For each={data()?.providers} fallback={<p class="text-sm text-text-muted">loading…</p>}>
          {(p) => (
            <section class="rounded-lg border border-border bg-surface p-4">
              <div class="mb-2 flex items-center justify-between">
                <div>
                  <span class="font-medium">{p.display_name}</span>
                  <code class="ml-2 rounded bg-bg px-1.5 py-0.5 text-xs text-text-muted">
                    {p.id}/model
                  </code>
                </div>
                <div class="flex items-center gap-2">
                  {p.notice_url && (
                    <a
                      href={p.notice_url}
                      target="_blank"
                      rel="noreferrer"
                      class="text-xs text-primary hover:underline"
                    >
                      get key ↗
                    </a>
                  )}
                  {p.category === "oauth" && (
                    <button
                      class="rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-black"
                      onClick={() => startOauth(p.id)}
                    >
                      connect
                    </button>
                  )}
                  <button
                    class="rounded-md border border-border px-2.5 py-1 text-xs text-text-muted hover:text-text"
                    onClick={() => setAddingFor(addingFor() === p.id ? null : p.id)}
                  >
                    + add key
                  </button>
                </div>
              </div>

              <Show when={addingFor() === p.id}>
                <div class="mb-3 flex gap-2 rounded-md border border-border bg-bg p-3">
                  <input
                    class="flex-1 rounded border border-border bg-surface px-2 py-1.5 text-sm"
                    placeholder="api key"
                    value={keyInput()}
                    onInput={(e) => setKeyInput(e.currentTarget.value)}
                  />
                  <input
                    class="w-36 rounded border border-border bg-surface px-2 py-1.5 text-sm"
                    placeholder="name (opt)"
                    value={nameInput()}
                    onInput={(e) => setNameInput(e.currentTarget.value)}
                  />
                  <button
                    class="rounded-md bg-primary px-3 py-1.5 text-sm text-white"
                    onClick={() => addKey(p.id)}
                  >
                    Save
                  </button>
                </div>
              </Show>

              <Show when={p.connections.length > 0}>
                <table class="w-full text-sm">
                  <tbody>
                    <For each={p.connections}>
                      {(c) => (
                        <tr class="border-t border-border/50">
                          <td class="py-2 pr-3">{c.name ?? c.data.apiKey ?? "—"}</td>
                          <td class="py-2 pr-3">
                            <span class={c.is_active ? "text-ok" : "text-text-muted"}>
                              {c.is_active ? "active" : "disabled"}
                            </span>
                          </td>
                          <td class="py-2 pr-3 text-xs text-text-muted">
                            {testResult()[c.id] ?? c.data.testStatus ?? ""}
                          </td>
                          <td class="py-2 text-right">
                            <button class="mr-3 text-xs text-primary hover:underline" onClick={() => test(c.id)}>
                              test
                            </button>
                            <button
                              class="mr-3 text-xs text-text-muted hover:underline"
                              onClick={() => toggle(c.id, !c.is_active)}
                            >
                              {c.is_active ? "disable" : "enable"}
                            </button>
                            <button class="text-xs text-danger hover:underline" onClick={() => remove(c.id)}>
                              delete
                            </button>
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </Show>

              <p class="mt-1 text-xs text-text-muted">
                {p.models.slice(0, 4).map((m) => m.id).join(", ")}
                {p.models.length > 4 ? ` +${p.models.length - 4} more` : ""}
              </p>
            </section>
          )}
        </For>
      </div>

      <section class="rounded-lg border border-border bg-surface p-4">
        <div class="mb-2 flex items-center justify-between">
          <h2 class="font-medium">Custom OpenAI-compatible nodes</h2>
          <button
            class="rounded-md border border-border px-2.5 py-1 text-xs text-text-muted hover:text-text"
            onClick={() => setShowNode(!showNode())}
          >
            + add node
          </button>
        </div>

        <Show when={showNode()}>
          <div class="mb-3 grid grid-cols-2 gap-2 rounded-md border border-border bg-bg p-3">
            <input
              class="rounded border border-border bg-surface px-2 py-1.5 text-sm"
              placeholder="prefix (e.g. local)"
              value={nodeForm().prefix}
              onInput={(e) => setNodeForm({ ...nodeForm(), prefix: e.currentTarget.value })}
            />
            <input
              class="rounded border border-border bg-surface px-2 py-1.5 text-sm"
              placeholder="base url (http://localhost:11434/v1)"
              value={nodeForm().base_url}
              onInput={(e) => setNodeForm({ ...nodeForm(), base_url: e.currentTarget.value })}
            />
            <input
              class="rounded border border-border bg-surface px-2 py-1.5 text-sm"
              placeholder="api key (opt)"
              value={nodeForm().api_key}
              onInput={(e) => setNodeForm({ ...nodeForm(), api_key: e.currentTarget.value })}
            />
            <div class="flex gap-2">
              <input
                class="flex-1 rounded border border-border bg-surface px-2 py-1.5 text-sm"
                placeholder="name (opt)"
                value={nodeForm().name}
                onInput={(e) => setNodeForm({ ...nodeForm(), name: e.currentTarget.value })}
              />
              <button class="rounded-md bg-primary px-3 py-1.5 text-sm text-white" onClick={addNode}>
                Save
              </button>
            </div>
          </div>
        </Show>

        <Show when={(data()?.nodes.length ?? 0) > 0} fallback={<p class="text-sm text-text-muted">None.</p>}>
          <table class="w-full text-sm">
            <tbody>
              <For each={data()?.nodes}>
                {(n) => (
                  <tr class="border-t border-border/50">
                    <td class="py-2 pr-3 font-mono text-xs">{n.data.prefix}/…</td>
                    <td class="py-2 pr-3 text-text-muted">{n.data.baseUrl}</td>
                    <td class="py-2 text-right">
                      <button
                        class="text-xs text-danger hover:underline"
                        onClick={async () => {
                          await api(`/providers/nodes/${n.id}`, { method: "DELETE" });
                          refetch();
                        }}
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
      </section>
    </div>
  );
}
