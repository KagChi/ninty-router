import { useNavigate } from "@solidjs/router";
import { Show, createSignal } from "solid-js";
import { api } from "~/lib/api";
import { Button, Icon, Input } from "~/components/ui";

export default function Login() {
  const navigate = useNavigate();
  const [password, setPassword] = createSignal("");
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  const submit = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api("/auth/login", {
        method: "POST",
        body: JSON.stringify({ password: password() }),
      });
      navigate("/endpoint", { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : "login failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="flex min-h-screen items-center justify-center bg-bg p-4">
      <form onSubmit={submit} class="card-elev w-80 border border-border p-8">
        <div class="mb-6 flex items-center gap-3">
          <img src="/logo.svg" alt="NintyRouter" width="36" height="36" class="rounded-[10px] shadow-[var(--shadow-warm)]" />
          <div>
            <h1 class="text-lg font-semibold tracking-tight text-text-main">NintyRouter</h1>
            <p class="text-xs text-text-muted">Dashboard login</p>
          </div>
        </div>
        <div class="mb-4">
          <Input
            type="password"
            placeholder="password"
            value={password()}
            onInput={setPassword}
          />
        </div>
        <Button type="submit" fullWidth loading={busy()}>
          Login
        </Button>
        <Show when={error()}>
          <p class="mt-3 flex items-center gap-1.5 text-sm text-danger">
            <Icon name="error" class="text-[16px]" /> {error()}
          </p>
        </Show>
      </form>
    </div>
  );
}
