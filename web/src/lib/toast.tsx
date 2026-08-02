import { For, Show, createSignal } from "solid-js";
import { Icon } from "~/components/ui";

/** 9router notificationStore 1:1: success/error/warning/info toasts,
 * fixed top-right, auto-dismiss 5s (error 8s), dismissible. */
export interface Toast {
  id: number;
  type: "success" | "error" | "warning" | "info";
  message: string;
  title?: string;
}

let idCounter = 0;
const [toasts, setToasts] = createSignal<Toast[]>([]);

function push(type: Toast["type"], message: string, title?: string) {
  const id = ++idCounter;
  setToasts((t) => [...t, { id, type, message, title }]);
  setTimeout(() => dismiss(id), type === "error" ? 8000 : 5000);
  return id;
}

export function dismiss(id: number) {
  setToasts((t) => t.filter((n) => n.id !== id));
}

export const toast = {
  success: (message: string, title?: string) => push("success", message, title),
  error: (message: string, title?: string) => push("error", message, title),
  warning: (message: string, title?: string) => push("warning", message, title),
  info: (message: string, title?: string) => push("info", message, title),
};

const META: Record<Toast["type"], { icon: string; cls: string }> = {
  success: { icon: "check_circle", cls: "text-green-500 border-green-500/30 bg-green-500/10" },
  error: { icon: "error", cls: "text-red-500 border-red-500/30 bg-red-500/10" },
  warning: { icon: "warning", cls: "text-yellow-500 border-yellow-500/30 bg-yellow-500/10" },
  info: { icon: "info", cls: "text-blue-500 border-blue-500/30 bg-blue-500/10" },
};

export function Toasts() {
  return (
    <div class="fixed right-4 top-4 z-[80] flex w-[min(92vw,380px)] flex-col gap-2">
      <For each={toasts()}>
        {(n) => (
          <div
            class={`flex items-start gap-2 rounded-lg border px-3 py-2 shadow-lg backdrop-blur ${META[n.type].cls}`}
            role="alert"
          >
            <Icon name={META[n.type].icon} class="mt-0.5 shrink-0 text-[16px]" />
            <div class="min-w-0 flex-1">
              <Show when={n.title}>
                <p class="text-xs font-semibold">{n.title}</p>
              </Show>
              <p class="text-xs leading-relaxed">{n.message}</p>
            </div>
            <button
              class="shrink-0 rounded p-0.5 opacity-70 hover:opacity-100"
              aria-label="Dismiss notification"
              onClick={() => dismiss(n.id)}
            >
              <Icon name="close" class="text-[14px]" />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
