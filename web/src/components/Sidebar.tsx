import { A, useLocation } from "@solidjs/router";
import { For, createSignal, onMount } from "solid-js";
import { cn, Icon } from "~/components/ui";

const NAV = [
  { href: "/endpoint", label: "Endpoint & Key", icon: "api" },
  { href: "/providers", label: "Providers", icon: "dns" },
  { href: "/combos", label: "Combos", icon: "layers" },
  { href: "/usage", label: "Usage", icon: "bar_chart" },
  { href: "/quota", label: "Quota Tracker", icon: "data_usage" },
];

const SYSTEM = [{ href: "/settings", label: "Settings", icon: "settings" }];

export default function Sidebar() {
  const location = useLocation();
  const [dark, setDark] = createSignal(true);

  onMount(() => {
    const stored = localStorage.getItem("theme");
    const isDark = stored ? stored === "dark" : true; // default dark
    setDark(isDark);
    document.documentElement.classList.toggle("dark", isDark);
    // material icons: show once font loaded
    document.fonts?.ready.then(() => document.documentElement.classList.add("fonts-loaded"));
  });

  const toggleTheme = () => {
    const next = !dark();
    setDark(next);
    document.documentElement.classList.toggle("dark", next);
    localStorage.setItem("theme", next ? "dark" : "light");
  };

  const isActive = (href: string) => location.pathname.startsWith(href);

  const NavLink = (item: { href: string; label: string; icon: string }) => (
    <A
      href={item.href}
      class={cn(
        "group flex items-center gap-3 rounded-lg px-3 py-1 no-underline transition-all",
        isActive(item.href)
          ? "bg-primary/10 text-primary"
          : "text-text-muted hover:bg-surface-2 hover:text-text-main"
      )}
    >
      <Icon
        name={item.icon}
        fill={isActive(item.href)}
        class={cn("text-[18px]", !isActive(item.href) && "transition-colors group-hover:text-primary")}
      />
      <span class="text-[13px] font-medium">{item.label}</span>
    </A>
  );

  return (
    <aside class="bg-vibrancy flex min-h-full w-72 flex-col border-r border-border-subtle backdrop-blur-xl transition-colors duration-300">
      {/* Traffic lights */}
      <div class="flex items-center gap-2 px-6 pt-5 pb-2">
        <div class="h-3 w-3 rounded-full bg-[#FF5F56]" />
        <div class="h-3 w-3 rounded-full bg-[#FFBD2E]" />
        <div class="h-3 w-3 rounded-full bg-[#27C93F]" />
      </div>

      {/* Logo */}
      <div class="flex flex-col gap-2 px-6 py-4">
        <A href="/" class="flex items-center gap-3 no-underline">
          <div class="flex size-9 items-center justify-center rounded-[10px] bg-gradient-to-br from-brand-500 to-brand-700 shadow-[var(--shadow-warm)]">
            <Icon name="hub" class="text-[20px] text-white" />
          </div>
          <div class="flex flex-col">
            <h1 class="text-lg font-semibold tracking-tight text-text-main">ninty-router</h1>
            <span class="text-xs text-text-muted">local ai router</span>
          </div>
        </A>
      </div>

      {/* Navigation */}
      <nav class="custom-scrollbar flex-1 space-y-0.5 overflow-y-auto px-4 py-2">
        <For each={NAV}>{NavLink}</For>

        <div class="mt-2 space-y-0.5 pt-3">
          <p class="mb-2 px-4 text-xs font-semibold uppercase tracking-wider text-text-muted/60">
            System
          </p>
          <For each={SYSTEM}>{NavLink}</For>
        </div>
      </nav>

      {/* Theme toggle */}
      <div class="border-t border-border-subtle px-6 py-3">
        <button
          class="flex w-full items-center gap-3 rounded-lg px-1 py-1 text-text-muted transition-colors hover:text-text-main"
          onClick={toggleTheme}
        >
          <Icon name={dark() ? "light_mode" : "dark_mode"} class="text-[18px]" />
          <span class="text-[13px] font-medium">{dark() ? "Light mode" : "Dark mode"}</span>
        </button>
      </div>
    </aside>
  );
}
