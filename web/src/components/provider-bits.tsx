import { Show, createSignal } from "solid-js";
import { A } from "@solidjs/router";
import { Badge, Card, Icon, Toggle, cn } from "~/components/ui";

export interface Connection {
  id: string;
  provider: string;
  auth_type: string;
  name: string | null;
  priority: number;
  is_active: boolean;
  data: Record<string, unknown> & {
    apiKey?: string;
    testStatus?: string;
    lastError?: string;
    unavailableUntil?: string;
  };
}

export interface ModelEntry {
  id: string;
  name: string;
  suggested?: boolean;
  custom?: boolean;
  disabled?: boolean;
  caps?: { vision: boolean; reasoning: boolean };
}

export interface Provider {
  id: string;
  alias: string;
  category: string;
  display_name: string;
  notice_url: string | null;
  color: string;
  text_icon: string;
  no_auth: boolean;
  models: ModelEntry[];
  connections: Connection[];
}

export interface Node {
  id: string;
  name: string | null;
  data: { prefix?: string; baseUrl?: string; apiKey?: string };
}

export function ProviderIcon(props: { id: string; color: string; textIcon: string; size?: number }) {
  const size = () => props.size ?? 32;
  const [err, setErr] = createSignal(false);
  return (
    <div
      class="flex shrink-0 items-center justify-center rounded-lg"
      style={{
        width: `${size()}px`,
        height: `${size()}px`,
        "background-color": `${props.color}${props.color.length > 7 ? "" : "15"}`,
      }}
    >
      <Show
        when={!err()}
        fallback={
          <span class="font-bold" style={{ color: props.color, "font-size": `${Math.max(10, size() * 0.38)}px` }}>
            {props.textIcon}
          </span>
        }
      >
        <img
          src={`/providers/${props.id}.png`}
          alt={props.id}
          width={size() - 2}
          height={size() - 2}
          class="max-w-full rounded-lg object-contain"
          loading="lazy"
          onError={() => setErr(true)}
        />
      </Show>
    </div>
  );
}

export function providerStats(p: Provider) {
  const total = p.connections.length;
  const connected = p.connections.filter((c) => c.is_active).length;
  const error = p.connections.filter((c) => c.data.lastError).length;
  const allDisabled = total > 0 && connected === 0;
  return { total, connected, error, allDisabled };
}

function StatusBadges(p: Provider) {
  const s = providerStats(p);
  return (
    <div class="flex min-w-0 flex-wrap items-center gap-1.5 text-xs">
      <Show when={s.allDisabled}>
        <Badge tone="neutral">
          <Icon name="pause_circle" class="text-[12px]" /> Disabled
        </Badge>
      </Show>
      <Show when={!s.allDisabled && p.no_auth}>
        <Badge tone="green">
          <span class="size-1.5 rounded-full bg-green-500" /> Ready
        </Badge>
      </Show>
      <Show when={!s.allDisabled && !p.no_auth}>
        <Show when={s.connected > 0}>
          <Badge tone="green">
            <span class="size-1.5 rounded-full bg-green-500" /> {s.connected} Connected
          </Badge>
        </Show>
        <Show when={s.error > 0}>
          <Badge tone="red">
            <span class="size-1.5 rounded-full bg-red-500" /> {s.error} Error
          </Badge>
        </Show>
        <Show when={s.connected === 0 && s.error === 0}>
          <span class="text-text-muted">No connections</span>
        </Show>
      </Show>
    </div>
  );
}

export function ProviderCard(props: { p: Provider; onToggle: (active: boolean) => void }) {
  const s = () => providerStats(props.p);
  return (
    <A href={`/providers/${props.p.id}`} class="group min-w-0 no-underline">
      <Card
        padding="xs"
        class={cn(
          "h-full cursor-pointer transition-colors hover:bg-black/[0.01] dark:hover:bg-white/[0.01]",
          s().allDisabled && "opacity-50"
        )}
      >
        <div class="flex min-w-0 items-center justify-between gap-3">
          <div class="flex min-w-0 items-center gap-3">
            <ProviderIcon id={props.p.id} color={props.p.color} textIcon={props.p.text_icon} />
            <div class="min-w-0">
              <h3 class="truncate font-semibold text-text-main">{props.p.display_name}</h3>
              <StatusBadges {...props.p} />
            </div>
          </div>
          <Show when={s().total > 0}>
            <div
              class="shrink-0 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100"
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                props.onToggle(s().allDisabled);
              }}
            >
              <Toggle checked={!s().allDisabled} onChange={() => {}} />
            </div>
          </Show>
        </div>
      </Card>
    </A>
  );
}
