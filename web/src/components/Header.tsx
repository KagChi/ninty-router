import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { useLocation, useNavigate } from "@solidjs/router";
import { api } from "~/lib/api";
import { Icon } from "~/components/ui";
import { toast } from "~/lib/toast";

/** Per-page meta (9router Header.js getPageInfo 1:1 strings). */
function pageInfo(pathname: string): { title: string; description: string; icon: string } {
  const rest = pathname.replace(/\/$/, "");
  if (rest.startsWith("/providers/")) {
    return { title: "Provider", description: "", icon: "dns" };
  }
  if (pathname.includes("/providers"))
    return { title: "Providers", description: "Manage your AI provider connections", icon: "dns" };
  if (pathname.includes("/combos"))
    return { title: "Combos", description: "Model combos with fallback", icon: "layers" };
  if (pathname.includes("/usage"))
    return {
      title: "Usage & Analytics",
      description: "Monitor your API usage, token consumption, and request logs",
      icon: "bar_chart",
    };
  if (pathname.includes("/quota"))
    return { title: "Quota Tracker", description: "Track and manage your API quota limits", icon: "data_usage" };
  if (pathname.includes("/endpoint"))
    return { title: "Endpoint", description: "API endpoint configuration", icon: "api" };
  if (pathname.includes("/settings"))
    return { title: "Settings", description: "Manage your preferences", icon: "settings" };
  return { title: "", description: "", icon: "" };
}

function MenuItem(props: {
  icon: string;
  label: string;
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      class={`flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm transition-colors hover:bg-surface-2 ${
        props.danger ? "text-red-500" : "text-text-main"
      }`}
      onClick={props.onClick}
    >
      <Icon name={props.icon} class="text-[18px]" />
      {props.label}
    </button>
  );
}

/** 9router Header + HeaderMenu 1:1: per-page icon/title/description,
 * dropdown menu (theme / shutdown / logout). */
export default function Header() {
  const location = useLocation();
  const navigate = useNavigate();
  const [menuOpen, setMenuOpen] = createSignal(false);
  const [shutdownConfirm, setShutdownConfirm] = createSignal(false);
  const [dark, setDark] = createSignal(
    typeof localStorage !== "undefined" ? localStorage.getItem("theme") !== "light" : true
  );
  let menuRef: HTMLDivElement | undefined;

  const info = () => pageInfo(location.pathname);

  const toggleTheme = () => {
    const next = !dark();
    setDark(next);
    document.documentElement.classList.toggle("dark", next);
    localStorage.setItem("theme", next ? "dark" : "light");
  };

  const logout = async () => {
    try {
      await api("/auth/logout", { method: "POST" });
      navigate("/login", { replace: true });
    } catch {
      toast.error("logout failed");
    }
  };

  const shutdown = async () => {
    setShutdownConfirm(false);
    try {
      await api("/version/shutdown", { method: "POST" });
      toast.warning("Router shutting down…");
    } catch {
      toast.error("shutdown failed");
    }
  };

  const onDocClick = (e: MouseEvent) => {
    if (menuRef && !menuRef.contains(e.target as globalThis.Node)) setMenuOpen(false);
  };
  onMount(() => document.addEventListener("click", onDocClick));
  onCleanup(() => document.removeEventListener("click", onDocClick));

  return (
    <header class="z-20 flex shrink-0 items-center justify-between gap-3 border-b border-border-subtle bg-surface/60 px-4 pb-2 pt-3 backdrop-blur-xl lg:px-8 lg:bg-transparent lg:backdrop-blur-none">
      <div class="flex min-w-0 items-center gap-2.5">
        <Show when={info().icon}>
          <div class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-surface-2">
            <Icon name={info().icon} class="text-[18px] text-text-muted" />
          </div>
        </Show>
        <div class="min-w-0">
          <h1 class="truncate text-base font-semibold tracking-tight text-text-main">
            {info().title}
          </h1>
          <Show when={info().description}>
            <p class="truncate text-xs text-text-muted">{info().description}</p>
          </Show>
        </div>
      </div>

      <div class="relative" ref={menuRef}>
        <button
          class="rounded-lg p-1.5 text-text-muted transition-colors hover:bg-surface-2 hover:text-text-main"
          title="Menu"
          onClick={() => setMenuOpen(!menuOpen())}
        >
          <Icon name="menu" class="text-[20px]" />
        </button>
        <Show when={menuOpen()}>
          <div class="absolute right-0 top-full z-50 mt-2 w-60 overflow-hidden rounded-xl border border-border bg-surface py-1 shadow-2xl">
            <MenuItem
              icon={dark() ? "light_mode" : "dark_mode"}
              label="Theme"
              onClick={() => {
                toggleTheme();
                setMenuOpen(false);
              }}
            />
            <MenuItem
              icon="power_settings_new"
              label="Shutdown"
              danger
              onClick={() => {
                setMenuOpen(false);
                setShutdownConfirm(true);
              }}
            />
            <MenuItem
              icon="logout"
              label="Logout"
              danger
              onClick={() => {
                setMenuOpen(false);
                void logout();
              }}
            />
          </div>
        </Show>
      </div>

      {/* shutdown confirm */}
      <Show when={shutdownConfirm()}>
        <div class="fixed inset-0 z-[70] flex items-center justify-center p-4">
          <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={() => setShutdownConfirm(false)} />
          <div class="relative w-full max-w-sm rounded-xl border border-border bg-surface p-5 shadow-2xl">
            <h3 class="font-semibold text-text-main">Close Router</h3>
            <p class="mt-1 text-sm text-text-muted">
              Stop the router process? All in-flight requests will fail.
            </p>
            <div class="mt-4 flex justify-end gap-2">
              <button
                class="rounded-lg border border-border px-3 py-1.5 text-sm text-text-muted hover:bg-surface-2"
                onClick={() => setShutdownConfirm(false)}
              >
                Cancel
              </button>
              <button
                class="rounded-lg bg-red-500 px-3 py-1.5 text-sm text-white hover:bg-red-600"
                onClick={shutdown}
              >
                Shutdown
              </button>
            </div>
          </div>
        </div>
      </Show>
    </header>
  );
}
