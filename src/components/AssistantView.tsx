import { useEffect, useRef, useState, type ReactNode } from "react";
import { Send, Trash2 } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { GlowRing } from "@/components/GlowRing";
import { Notice } from "@/components/PageChrome";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import type { VoiceState } from "@/hooks/useVoice";
import type { VoiceAccount } from "@/lib/api";
import { readyProvider, recoveryFor, settingsLabel, type SettingsDestination } from "@/lib/flow";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

function RichText({ text }: { text: string }) {
  const blocks = text.trim().split(/\n{2,}/);
  return (
    <div className="space-y-3 break-words">
      {blocks.map((block, i) => (
        <p key={i} className="leading-6">
          {inline(block)}
        </p>
      ))}
    </div>
  );
}

function inline(block: string): ReactNode[] {
  const parts = block.split(/(\*\*[^*]+\*\*|https?:\/\/[^\s)]+)/g);
  return parts.map((part, i) => {
    if (part.startsWith("**") && part.endsWith("**")) {
      return <strong key={i}>{part.slice(2, -2)}</strong>;
    }
    if (/^https?:\/\//.test(part)) {
      return (
        <button key={i} type="button" className="min-h-12 break-all text-left underline" onClick={() => void openUrl(part)}>
          {part}
        </button>
      );
    }
    return <span key={i}>{part}</span>;
  });
}

function sessionClock(openSinceMs: number | null | undefined) {
  if (!openSinceMs) return "";
  const seconds = Math.max(0, Math.floor((Date.now() - openSinceMs) / 1000));
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

export function AssistantView({
  voice,
  wakeTrained,
  accounts,
  onOpenSettings,
}: {
  voice: VoiceState;
  wakeTrained: boolean;
  accounts: VoiceAccount[];
  onOpenSettings: (where: SettingsDestination) => void;
}) {
  const t = useT();
  const scroller = useRef<HTMLDivElement>(null);
  const [draft, setDraft] = useState("");
  const clock = sessionClock(voice.usage?.openSinceMs);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [voice.rows, voice.busy, voice.sending]);

  const recovery = voice.lastError ? recoveryFor(voice.lastError) : null;
  const provider = readyProvider(voice.provider, accounts);
  const inCall = voice.provider.length > 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="shrink-0 px-1 pb-3">
        <div className="flex items-start justify-between gap-3">
          <h1 className="text-2xl leading-tight font-medium tracking-tight">{t("Talk")}</h1>
          {clock ? <p className="pt-1 text-sm tabular-nums text-muted-foreground">{clock}</p> : null}
        </div>
        <p className="mt-2 inline-flex min-h-8 items-center border border-border px-2 text-sm text-foreground">
          {inCall ? `${t("Using")} ${provider}` : accounts.length === 0 ? t("No voice yet") : provider}
        </p>
      </header>

      <div className="mb-3 shrink-0 border border-border px-3 py-2">
        <label className="flex min-h-12 items-center justify-between gap-3">
          <span>
            <span className="block text-base text-foreground">{t("Listen")}</span>
            <span className="block text-sm leading-5 text-muted-foreground">
              {voice.wakeEnabled ? t("On") : t("Off")}
              {wakeTrained ? `. ${t("Hands-free stays on after you leave")}` : `. ${t("Hey Grok is not enrolled yet.")}`}
              {clock ? ` · ${clock}` : ""}
            </span>
          </span>
          <Switch checked={voice.wakeEnabled} onCheckedChange={(v) => void voice.toggleWake(v)} />
        </label>
        {!wakeTrained && (
          <Button type="button" variant="outline" className="mt-2 w-full" onClick={() => onOpenSettings("wake")}>
            {t("Set up Hey Grok")}
          </Button>
        )}
      </div>

      {voice.lastError && (
        <Notice
          title={voice.lastError}
          body={recovery ? t(recovery.hint) : t("Try again. If it keeps happening, open Settings, then Debug.")}
          actionLabel={recovery?.go ? t(settingsLabel(recovery.go)) : undefined}
          onAction={recovery?.go ? () => onOpenSettings(recovery.go!) : undefined}
          onDismiss={voice.dismissError}
        />
      )}

      <div ref={scroller} className="min-h-0 flex-1 space-y-3 overflow-y-auto px-1">
        {voice.rows.length === 0 ? (
          <div className="py-6">
            <p className="text-base leading-6 text-foreground">{t("Say “Hey Grok”, tap the microphone, or type when you can't.")}</p>
            {accounts.length === 0 && (
              <Button type="button" className="mt-4" onClick={() => onOpenSettings("keys")}>
                {t("Add a voice")}
              </Button>
            )}
          </div>
        ) : (
          <>
            <div className="flex justify-end">
              <Button size="sm" variant="ghost" onClick={voice.clearTranscript}>
                <Trash2 /> {t("Clear")}
              </Button>
            </div>
            {voice.rows.map((r) => (
              <div
                key={`${r.role}-${r.id}`}
                className={cn("max-w-[88%] px-0 py-1 text-base leading-6", r.role === "user" ? "ml-auto text-right" : "mr-auto text-left")}
              >
                <p className="mb-1 text-xs text-muted-foreground">{r.role === "user" ? t("you") : t("grok")}</p>
                {r.role === "user" || !r.text ? (
                  <p className="whitespace-pre-wrap text-foreground">{r.text || "…"}</p>
                ) : (
                  <RichText text={r.text} />
                )}
              </div>
            ))}
          </>
        )}
        {(voice.sending || voice.busy) && (
          <div className="mr-auto py-1 text-base leading-6">
            <p className="mb-1 text-xs text-muted-foreground">{t("grok")}</p>
            <p className="text-muted-foreground">{voice.busy || t("Searching the web…")}</p>
          </div>
        )}
      </div>

      {voice.handoff && (
        <div className="mb-3 border border-border px-3 py-3">
          <p className="text-sm leading-6 text-foreground">{voice.handoff.summary}</p>
          <p className="mt-1 text-sm leading-5 text-muted-foreground">{t("Nothing is sent until you confirm in the other app.")}</p>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button type="button" onClick={voice.confirmHandoff}>
              {t("Send the draft")}
            </Button>
            <Button type="button" variant="outline" onClick={voice.dismissHandoff}>
              {t("Not now")}
            </Button>
          </div>
        </div>
      )}

      <form
        className="mt-auto flex shrink-0 items-center gap-2 border-t border-border pt-3"
        onSubmit={(e) => {
          e.preventDefault();
          const text = draft;
          setDraft("");
          void voice.sendText(text);
        }}
      >
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder={t("Type a message")}
          aria-label={t("Type a message")}
          enterKeyHint="send"
          className="h-12 min-w-0 flex-1 rounded-[16px] border border-border bg-transparent px-4 text-base text-foreground outline-none placeholder:text-muted-foreground focus:border-foreground"
        />
        <button
          type="submit"
          disabled={voice.sending || draft.trim().length === 0}
          aria-label={t("Send")}
          className="grid size-12 shrink-0 place-items-center rounded-full border border-foreground text-foreground disabled:opacity-40"
        >
          <Send className="size-5" strokeWidth={1.25} />
        </button>
      </form>
      <div className="shrink-0 pt-4">
        <GlowRing phase={voice.phase} level={voice.level} onPress={() => void voice.toggleSession()} />
      </div>
    </div>
  );
}
