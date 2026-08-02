import { Show, type JSX, children } from "solid-js";

export const cn = (...parts: (string | false | null | undefined)[]) =>
  parts.filter(Boolean).join(" ");

export const Icon = (props: { name: string; class?: string; fill?: boolean }) => (
  <span class={cn("material-symbols-outlined", props.fill && "fill-1", props.class)}>
    {props.name}
  </span>
);

// ---------- Button (9router variants/sizes 1:1) ----------

const btnVariants: Record<string, string> = {
  primary:
    "bg-brand-500 hover:bg-brand-600 text-white shadow-sm disabled:bg-surface-3 disabled:text-text-muted",
  secondary:
    "bg-surface-2 hover:bg-surface-3 text-text-main border border-border disabled:opacity-50",
  outline: "border border-border text-text-main hover:bg-surface-2 hover:border-brand-500/40",
  ghost: "text-text-muted hover:bg-surface-2 hover:text-text-main",
  danger: "bg-red-500 hover:bg-red-600 text-white shadow-sm disabled:bg-surface-3 disabled:text-text-muted",
  success: "bg-green-600 hover:bg-green-700 text-white shadow-sm disabled:bg-surface-3 disabled:text-text-muted",
};

const btnSizes: Record<string, string> = {
  sm: "h-7 px-3 text-xs rounded-[8px]",
  md: "h-9 px-4 text-sm rounded-[10px]",
  lg: "h-11 px-6 text-sm rounded-[10px]",
};

export function Button(props: {
  variant?: keyof typeof btnVariants;
  size?: keyof typeof btnSizes;
  icon?: string;
  iconRight?: string;
  disabled?: boolean;
  loading?: boolean;
  fullWidth?: boolean;
  class?: string;
  type?: "button" | "submit";
  onClick?: (e: MouseEvent) => void;
  children?: JSX.Element;
}) {
  return (
    <button
      type={props.type ?? "button"}
      class={cn(
        "inline-flex items-center justify-center gap-2 font-semibold transition-all duration-150 ease-out cursor-pointer",
        "active:scale-[0.97] disabled:opacity-50 disabled:cursor-not-allowed disabled:active:scale-100",
        btnVariants[props.variant ?? "primary"],
        btnSizes[props.size ?? "md"],
        props.fullWidth && "w-full",
        props.class
      )}
      disabled={props.disabled || props.loading}
      onClick={props.onClick}
    >
      <Show when={props.loading} fallback={props.icon && <Icon name={props.icon} class="text-[18px]" />}>
        <Icon name="progress_activity" class="animate-spin text-[18px]" />
      </Show>
      {props.children}
      <Show when={props.iconRight && !props.loading}>
        <Icon name={props.iconRight!} class="text-[18px]" />
      </Show>
    </button>
  );
}

// ---------- Card (9router Card 1:1 + Section/Row) ----------

const cardPaddings: Record<string, string> = {
  none: "",
  xs: "p-3",
  sm: "p-4",
  md: "p-6",
  lg: "p-8",
};

export function Card(props: {
  title?: string;
  subtitle?: string;
  icon?: string;
  action?: JSX.Element;
  padding?: keyof typeof cardPaddings;
  hover?: boolean;
  elev?: boolean;
  class?: string;
  children?: JSX.Element;
}) {
  return (
    <div
      class={cn(
        "bg-surface border border-border-subtle",
        props.elev
          ? "rounded-[14px] shadow-[var(--shadow-elev)]"
          : "rounded-[14px] shadow-[var(--shadow-soft)]",
        props.hover &&
          "hover:shadow-[var(--shadow-warm)] hover:border-brand-500/30 transition-all cursor-pointer",
        cardPaddings[props.padding ?? "md"],
        props.class
      )}
    >
      <Show when={props.title || props.action}>
        <div class="mb-4 flex items-center justify-between">
          <div class="flex items-center gap-3">
            <Show when={props.icon}>
              <div class="rounded-[10px] bg-bg p-2 text-text-muted">
                <Icon name={props.icon!} class="text-[20px]" />
              </div>
            </Show>
            <div>
              <Show when={props.title}>
                <h3 class="font-semibold text-text-main">{props.title}</h3>
              </Show>
              <Show when={props.subtitle}>
                <p class="text-sm text-text-muted">{props.subtitle}</p>
              </Show>
            </div>
          </div>
          {props.action}
        </div>
      </Show>
      {props.children}
    </div>
  );
}

export function CardSection(props: { class?: string; children?: JSX.Element }) {
  return (
    <div class={cn("rounded-[10px] border border-border-subtle bg-bg p-4", props.class)}>
      {props.children}
    </div>
  );
}

export function CardRow(props: { class?: string; children?: JSX.Element }) {
  return (
    <div
      class={cn(
        "-mx-3 border-b border-border-subtle p-3 px-3 transition-colors last:border-b-0 hover:bg-surface-2/50",
        props.class
      )}
    >
      {props.children}
    </div>
  );
}

// ---------- Input / Select / Textarea ----------

export function Input(props: {
  value?: string | number;
  type?: string;
  placeholder?: string;
  class?: string;
  min?: string;
  step?: string;
  onInput?: (v: string) => void;
}) {
  return (
    <input
      type={props.type ?? "text"}
      value={props.value ?? ""}
      placeholder={props.placeholder}
      min={props.min}
      step={props.step}
      onInput={(e) => props.onInput?.(e.currentTarget.value)}
      class={cn(
        "h-9 w-full rounded-[10px] border border-border bg-bg px-3 text-sm text-text-main",
        "placeholder:text-text-subtle focus:border-brand-500/50 focus:outline-none focus:shadow-[var(--shadow-focus)]",
        props.class
      )}
    />
  );
}

export function Select(props: {
  value?: string;
  class?: string;
  onChange?: (v: string) => void;
  children?: JSX.Element;
}) {
  return (
    <select
      value={props.value}
      onChange={(e) => props.onChange?.(e.currentTarget.value)}
      class={cn(
        "h-9 rounded-[10px] border border-border bg-bg px-3 text-sm text-text-main",
        "focus:border-brand-500/50 focus:outline-none focus:shadow-[var(--shadow-focus)]",
        props.class
      )}
    >
      {props.children}
    </select>
  );
}

export function Textarea(props: {
  value?: string;
  rows?: number;
  placeholder?: string;
  class?: string;
  onInput?: (v: string) => void;
}) {
  return (
    <textarea
      rows={props.rows ?? 3}
      value={props.value ?? ""}
      placeholder={props.placeholder}
      onInput={(e) => props.onInput?.(e.currentTarget.value)}
      class={cn(
        "w-full rounded-[10px] border border-border bg-bg px-3 py-2 text-sm text-text-main",
        "placeholder:text-text-subtle focus:border-brand-500/50 focus:outline-none focus:shadow-[var(--shadow-focus)]",
        props.class
      )}
    />
  );
}

// ---------- SegmentedControl (9router 1:1) ----------

export function SegmentedControl<T extends string>(props: {
  options: { value: T; label: string; icon?: string }[];
  value: T;
  onChange: (v: T) => void;
  size?: "sm" | "md";
}) {
  return (
    <div class="inline-flex items-center gap-0.5 rounded-[10px] border border-border-subtle bg-surface-2 p-0.5">
      {props.options.map((o) => (
        <button
          class={cn(
            "inline-flex items-center gap-1.5 rounded-[8px] font-medium transition-all",
            props.size === "sm" ? "px-2 py-1 text-xs" : "px-3 py-1.5 text-[13px]",
            props.value === o.value
              ? "bg-surface text-text-main shadow-[var(--shadow-soft)]"
              : "text-text-muted hover:text-text-main"
          )}
          onClick={() => props.onChange(o.value)}
        >
          {o.icon && <Icon name={o.icon} class="text-[16px]" />}
          {o.label}
        </button>
      ))}
    </div>
  );
}

// ---------- Badge ----------

const badgeTones: Record<string, string> = {
  green: "bg-green-500/10 text-green-600 dark:text-green-400 border-green-500/30",
  red: "bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/30",
  amber: "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/30",
  blue: "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/30",
  neutral: "bg-surface-2 text-text-muted border-border",
  brand: "bg-brand-500/10 text-brand-500 border-brand-500/30",
};

export function Badge(props: { tone?: keyof typeof badgeTones; children?: JSX.Element }) {
  return (
    <span
      class={cn(
        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium",
        badgeTones[props.tone ?? "neutral"]
      )}
    >
      {props.children}
    </span>
  );
}

// ---------- Modal ----------

export function Modal(props: {
  open: boolean;
  title: string;
  onClose: () => void;
  children?: JSX.Element;
  footer?: JSX.Element;
}) {
  return (
    <Show when={props.open}>
      <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4 backdrop-blur-sm"
        onClick={(e) => e.target === e.currentTarget && props.onClose()}
      >
        <div class="card-elev w-full max-w-lg border border-border p-6">
          <div class="mb-4 flex items-center justify-between">
            <h3 class="text-lg font-semibold text-text-main">{props.title}</h3>
            <button
              class="rounded-lg p-1 text-text-muted hover:bg-surface-2 hover:text-text-main"
              onClick={props.onClose}
            >
              <Icon name="close" class="text-[20px]" />
            </button>
          </div>
          {props.children}
          <Show when={props.footer}>
            <div class="mt-6 flex justify-end gap-2">{props.footer}</div>
          </Show>
        </div>
      </div>
    </Show>
  );
}

// ---------- PageHeader ----------

export function PageHeader(props: {
  title: string;
  subtitle?: string;
  actions?: JSX.Element;
}) {
  return (
    <div class="mb-6 flex flex-wrap items-center justify-between gap-3">
      {/* Title/subtitle hidden on desktop — sticky Header owns page title (9router 1:1) */}
      <div class="lg:hidden">
        <h1 class="text-2xl font-semibold tracking-tight text-text-main">{props.title}</h1>
        <Show when={props.subtitle}>
          <p class="mt-0.5 text-sm text-text-muted">{props.subtitle}</p>
        </Show>
      </div>
      {props.actions}
    </div>
  );
}

// ---------- Toggle (checkbox → switch look) ----------

export function Toggle(props: { checked: boolean; onChange: () => void; label?: string }) {
  const c = children(() => props.label);
  return (
    <button
      role="switch"
      aria-checked={props.checked}
      onClick={props.onChange}
      class={cn(
        "relative h-5 w-9 shrink-0 rounded-full transition-colors",
        props.checked ? "bg-brand-500" : "bg-surface-3"
      )}
    >
      <span
        class={cn(
          "absolute top-0.5 size-4 rounded-full bg-white shadow transition-all",
          props.checked ? "left-[18px]" : "left-0.5"
        )}
      />
      {c()}
    </button>
  );
}

// ---------- Skeletons (9router Loading.js 1:1) ----------

export function Skeleton(props: { class?: string }) {
  return <div class={cn("animate-pulse rounded-[10px] bg-surface-2", props.class)} />;
}

export function CardSkeleton() {
  return (
    <div class="rounded-[14px] border border-border-subtle bg-surface p-6 shadow-[var(--shadow-soft)]">
      <div class="mb-4 flex items-center justify-between">
        <Skeleton class="h-4 w-24" />
        <Skeleton class="size-10 rounded-[10px]" />
      </div>
      <Skeleton class="mb-2 h-8 w-16" />
      <Skeleton class="h-3 w-20" />
    </div>
  );
}

export function TableSkeleton(props: { rows?: number }) {
  return (
    <div class="flex flex-col gap-2">
      {Array.from({ length: props.rows ?? 4 }, () => (
        <Skeleton class="h-10 w-full" />
      ))}
    </div>
  );
}
