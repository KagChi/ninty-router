import { For, Show, createResource, createSignal } from "solid-js";
import { A, useParams } from "@solidjs/router";
import { api } from "~/lib/api";
import { Badge, Button, Card, CardSection, Icon, Input, Modal, cn, CardSkeleton } from "~/components/ui";
import { ProviderIcon, type Connection, type Provider } from "~/components/provider-bits";

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

            <Card title="Connections" icon="dns">
              <Show
                when={p().connections.length > 0}
                fallback={
                  <p class="py-4 text-center text-sm text-text-muted">
                    No connections yet — add an API key{p().category === "oauth" ? " or connect via OAuth" : ""}.
                  </p>
                }
              >
                <div class="overflow-x-auto">
                  <table class="w-full text-sm">
                  <thead>
                    <tr class="border-b border-border-subtle text-left text-xs font-semibold text-text-muted">
                      <th class="py-2 pr-3">Account</th>
                      <th class="py-2 pr-3">Status</th>
                      <th class="py-2 pr-3">Test</th>
                      <th class="py-2" />
                    </tr>
                  </thead>
                  <tbody>
                    <For each={p().connections}>
                      {(c) => (
                        <tr class="border-b border-border-subtle last:border-b-0">
                          <td class="max-w-48 truncate py-2.5 pr-3 font-medium">{connLabel(c)}</td>
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
                      )}
                    </For>
                  </tbody>
                </table>

                </div>
              </Show>
            </Card>

            <Card title={`Models (${p().models.length})`} icon="model_training">
              <div class="flex flex-wrap gap-1.5">
                <For each={p().models}>
                  {(m) => (
                    <code class="rounded-[8px] bg-bg px-2 py-1 font-mono text-xs text-text-muted">
                      {m.id}
                    </code>
                  )}
                </For>
              </div>
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
