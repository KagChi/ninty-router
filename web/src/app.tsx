import { Router } from "@solidjs/router";
import { FileRoutes } from "@solidjs/start/router";
import { Suspense, createResource, createSignal, Show } from "solid-js";
import { useLocation, useNavigate } from "@solidjs/router";
import "./app.css";
import { api, type AuthStatus } from "~/lib/api";
import { Skeleton, Icon } from "~/components/ui";
import Sidebar from "~/components/Sidebar";
import { Toasts } from "~/lib/toast";
import Header from "~/components/Header";

function Shell(props: { children: unknown }) {
  const location = useLocation();
  const navigate = useNavigate();
  const [drawerOpen, setDrawerOpen] = createSignal(false);
  const [auth] = createResource(async () => {
    try {
      return await api<AuthStatus>("/auth/status");
    } catch {
      return { authenticated: true, require_login: false, password_set: false };
    }
  });

  const isLoginPage = () => location.pathname.startsWith("/login");

  return (
    <Show
      when={!auth.loading}
      fallback={
        <div class="flex h-screen w-full gap-6 overflow-hidden bg-bg p-6">
          <Skeleton class="hidden h-full w-72 shrink-0 rounded-[14px] lg:block" />
          <div class="flex flex-1 flex-col gap-4">
            <Skeleton class="h-8 w-48" />
            <Skeleton class="h-40 w-full rounded-[14px]" />
            <Skeleton class="h-40 w-full rounded-[14px]" />
          </div>
        </div>
      }
    >
      <Show
        when={!isLoginPage()}
        fallback={props.children}
      >
        {(() => {
          const a = auth();
          if (a?.require_login && !a.authenticated) {
            navigate("/login", { replace: true });
            return null;
          }
          return (
            <div class="flex h-screen w-full overflow-hidden bg-bg">
              <Toasts />
              {/* Desktop sidebar */}
              <div class="hidden lg:flex">
                <Sidebar />
              </div>

              {/* Mobile drawer */}
              <Show when={drawerOpen()}>
                <div
                  class="fixed inset-0 z-40 bg-black/20 lg:hidden"
                  onClick={() => setDrawerOpen(false)}
                />
              </Show>
              <div
                class={`fixed inset-y-0 left-0 z-50 transform transition-transform duration-300 ease-in-out lg:hidden ${
                  drawerOpen() ? "translate-x-0" : "-translate-x-full"
                }`}
              >
                <Sidebar onClose={() => setDrawerOpen(false)} />
              </div>

              <main class="flex-1 overflow-y-auto">
                {/* Desktop header (9router Header.js 1:1) */}
                <div class="sticky top-0 z-20 hidden bg-bg/80 backdrop-blur-xl lg:block">
                  <Header />
                </div>
                {/* Mobile top bar */}
                <div class="flex items-center gap-3 border-b border-border-subtle px-4 py-3 lg:hidden">
                  <button
                    class="rounded-lg p-1.5 text-text-muted transition-colors hover:bg-surface-2 hover:text-text-main"
                    onClick={() => setDrawerOpen(true)}
                    aria-label="Open menu"
                  >
                    <Icon name="menu" class="text-[22px]" />
                  </button>
                  <div class="flex items-center gap-2.5">
                    <img src="/logo.svg" alt="NintyRouter" width="28" height="28" class="rounded-[8px] shadow-[var(--shadow-warm)]" />
                    <h1 class="text-base font-semibold tracking-tight text-text-main">NintyRouter</h1>
                  </div>
                </div>
                <div class="mx-auto max-w-5xl p-4 sm:p-6 lg:p-8">
                  <Suspense>{props.children}</Suspense>
                </div>
              </main>
            </div>
          );
        })()}
      </Show>
    </Show>
  );
}

export default function App() {
  return (
    <Router root={(props) => <Shell>{props.children}</Shell>}>
      <FileRoutes />
    </Router>
  );
}
