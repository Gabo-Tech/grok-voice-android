import type { VoiceAccount } from "@/lib/api";

export type SettingsDestination = "menu" | "keys" | "packs" | "wake" | "quiet" | "spend" | "model" | "voice" | "debug";

export function providerLabel(id: string): string {
  if (id === "local" || id === "Local") return "On device";
  if (id === "xai" || id === "xAI") return "xAI";
  if (id === "openai" || id === "OpenAI") return "OpenAI";
  if (id === "onemin" || id === "1min.ai") return "1min.ai";
  return id || "Voice";
}

/** Indexes Talk tries. Local moves first only when that preference is on and the packs are ready. */
export function attemptOrder(providers: string[], preferLocal: boolean, packsReady: boolean): number[] {
  const order = providers.map((_, index) => index);
  if (!(preferLocal && packsReady)) return order;
  const localAt = providers.findIndex((provider) => provider === "local");
  if (localAt < 0) return order;
  const next = order.filter((index) => index !== localAt);
  next.unshift(localAt);
  return next;
}

export function chainPlace(index: number, total: number): string {
  if (total <= 1) return "Only voice";
  if (index === 0) return "Tried first";
  if (index === total - 1) return "Last fallback";
  return "Then this";
}

export function readyProvider(
  live: string,
  accounts: VoiceAccount[],
  preferLocal = false,
  packsReady = false,
): string {
  if (live) return providerLabel(live);
  const providers = accounts.map((account) => account.provider);
  const preferred = preferLocal && packsReady ? providers.find((provider) => provider === "local") : undefined;
  const first = preferred ?? providers[0];
  if (!first) return "No voice yet";
  return `${providerLabel(first)} first`;
}

export function packsReadyForTalk(packs: { id: string; ready: boolean; selected?: boolean; kind?: string }[]): boolean {
  const ready = (id: string) => packs.some((pack) => pack.id === id && pack.ready);
  const selected = packs.find((pack) => pack.selected && (pack.kind === "llm" || pack.kind === "custom"));
  const voice = ready("tts") || packs.some((pack) => pack.kind === "phonon" && pack.ready);
  return ready("stt") && voice && Boolean(selected?.ready);
}

export function recoveryFor(message: string): { hint: string; go?: SettingsDestination } | null {
  const text = message.toLowerCase();
  if (/microphone|permission/.test(text)) {
    return {
      hint: "Allow the microphone in Android Settings, under Apps, then Grok Voice, then Permissions. Come back and try again.",
    };
  }
  if (/pack|not downloaded|cannot load|could not load|low on memory/.test(text)) {
    return {
      hint: "Download the missing packs, or put a cloud key above On device. Talk tries the list in order.",
      go: "voice",
    };
  }
  if (/no voice key|add a voice key|add one in setup|voice key first/.test(text)) {
    return {
      hint: "Add xAI, OpenAI, 1min.ai, or On device. The first one in the list is the one Talk tries.",
      go: "voice",
    };
  }
  if (/offline|couldn't reach|could not reach|timed out|network|connection/.test(text)) {
    return {
      hint: "Check the connection. On device still answers offline after its packs are on the phone.",
      go: "voice",
    };
  }
  if (/wake word/.test(text)) {
    return {
      hint: "Record Hey Grok, or leave it and tap the circle whenever you want to talk.",
      go: "wake",
    };
  }
  if (/voice limit|minutes is used|budget/.test(text)) {
    return {
      hint: "Today's limit is used up. Raise it, or turn the budget off.",
      go: "spend",
    };
  }
  return null;
}

export function settingsLabel(go: SettingsDestination): string {
  switch (go) {
    case "keys":
    case "packs":
    case "model":
    case "voice":
      return "Open voice and AI";
    case "wake":
      return "Set up Hey Grok";
    case "spend":
      return "Open daily budget";
    case "debug":
      return "Open debug";
    default:
      return "Open settings";
  }
}

export async function shareText(title: string, text: string): Promise<"shared" | "copied" | "cancelled" | "failed"> {
  const body = [title.trim(), text.trim()].filter(Boolean).join("\n\n");
  if (!body) return "failed";
  if (typeof navigator.share === "function") {
    try {
      await navigator.share({ title: title.trim() || "Grok Voice", text: body });
      return "shared";
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") return "cancelled";
    }
  }
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(body);
      return "copied";
    }
  } catch {
    // Some WebViews block the async clipboard. The selection fallback is next.
  }
  const area = document.createElement("textarea");
  area.value = body;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  const ok = document.execCommand("copy");
  area.remove();
  return ok ? "copied" : "failed";
}
