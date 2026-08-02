import { Router } from "@solidjs/router";
import { FileRoutes } from "@solidjs/start/router";
import { Suspense, createResource, Show } from "solid-js";
import { useLocation, useNavigate } from "@solidjs/router";
import "./app.css";
import { api, type AuthStatus } from "~/lib/api";
import { Skeleton } from "~/components/ui";
import Sidebar from "~/components/Sidebar";

function Shell(props: { children: unknown }) {
  const location = useLocation();
  const navigate = useNavigate();
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
          <Skeleton class="h-full w-72 shrink-0 rounded-[14px]" />
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
              <Sidebar />
              <main class="flex-1 overflow-y-auto">
                <div class="mx-auto max-w-5xl p-6 lg:p-8">
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
