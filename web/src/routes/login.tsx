import { useNavigate } from "@solidjs/router";
import { createSignal } from "solid-js";
import { api } from "~/lib/api";

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
    <div class="flex min-h-screen items-center justify-center">
      <form
        onSubmit={submit}
        class="w-80 rounded-lg border border-border bg-surface p-6"
      >
        <h1 class="mb-1 text-lg font-semibold">ninty-router</h1>
        <p class="mb-4 text-sm text-text-muted">Dashboard login</p>
        <input
          type="password"
          placeholder="password"
          value={password()}
          onInput={(e) => setPassword(e.currentTarget.value)}
          class="mb-3 w-full rounded-md border border-border bg-bg px-3 py-2 text-sm outline-none focus:border-primary"
          autofocus
        />
        <button
          type="submit"
          disabled={busy()}
          class="w-full rounded-md bg-primary px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
        >
          {busy() ? "…" : "Login"}
        </button>
        {error() && <p class="mt-3 text-sm text-danger">{error()}</p>}
      </form>
    </div>
  );
}
