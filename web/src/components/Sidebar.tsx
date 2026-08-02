import { A, useLocation } from "@solidjs/router";
import { For } from "solid-js";

const NAV = [
  { href: "/endpoint", label: "Endpoint & Keys", icon: "key" },
  { href: "/providers", label: "Providers", icon: "hub" },
  { href: "/combos", label: "Combos", icon: "alt_route" },
  { href: "/usage", label: "Usage", icon: "monitoring" },
  { href: "/quota", label: "Quota", icon: "speed" },
  { href: "/settings", label: "Settings", icon: "settings" },
];

export default function Sidebar() {
  const location = useLocation();
  return (
    <aside class="w-56 shrink-0 border-r border-border bg-surface p-4">
      <div class="mb-6 px-2 text-lg font-semibold">
        ninty<span class="text-primary">-router</span>
      </div>
      <nav class="flex flex-col gap-1">
        <For each={NAV}>
          {(item) => (
            <A
              href={item.href}
              class={`flex items-center gap-3 rounded-md px-3 py-2 text-sm no-underline transition-colors ${
                location.pathname.startsWith(item.href)
                  ? "bg-surface-2 text-text"
                  : "text-text-muted hover:bg-surface-2 hover:text-text"
              }`}
            >
              <span class="icon">{item.icon}</span>
              {item.label}
            </A>
          )}
        </For>
      </nav>
    </aside>
  );
}
