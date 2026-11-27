import { useEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";
import { ChevronDown, Info, Mic, Send, Trash2 } from "lucide-react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { GlowRing } from "@/components/GlowRing";
import { Notice } from "@/components/PageChrome";
import { Button } from "@/components/ui/button";
import type { VoiceState } from "@/hooks/useVoice";
import { api, type Settings, type VoiceAccount } from "@/lib/api";
import { packsReadyForTalk, readyProvider, recoveryFor, settingsLabel, type SettingsDestination } from "@/lib/flow";
import { useT } from "@/lib/i18n";
import { safeHttps } from "@/lib/links";
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
    if (/^https?:\/\//.test(part) && safeHttps(part)) {
      return (
        <button key={i} type="button" className="min-h-12 break-all text-left underline" onClick={() => void openUrl(part)}>
          {part}
        </button>
      );
    }
    return <span key={i}>{part}</span>;
  });
}

function sampleBubbles(): { id: string; role: "user" | "assistant"; text: string }[] {
  return [
    { id: "sample-user", role: "user", text: "What did I spend on the card this month?" },
    { id: "sample-grok", role: "assistant", text: "Housing was the largest expense. The Swisscard payment is still owed, separate from that spending." },
  ];
}

function previewQuery(name: string): boolean {
  if (typeof window === "undefined") return false;
  return new URLSearchParams(window.location.search).get(name) === "1";
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
  const draftBox = useRef<HTMLTextAreaElement>(null);
  const [draft, setDraft] = useState("");
  const [preferLocal, setPreferLocal] = useState(false);
  const [packsReady, setPacksReady] = useState(false);
  const [focus, setFocus] = useState(() => sessionStorage.getItem("grok-talk-focus") === "1" || previewQuery("focus"));
  const [micOpen, setMicOpen] = useState(() => localStorage.getItem("grok-mic-open") === "1" || previewQuery("mic"));
  const [aboutListen, setAboutListen] = useState(false);
  const [listenHint, setListenHint] = useState(false);
  const [focusHint, setFocusHint] = useState(false);
  const [clearAsk, setClearAsk] = useState(() => previewQuery("clear"));
  const lastTap = useRef(0);
  const clock = sessionClock(voice.usage?.openSinceMs);
  const live = ["connecting", "listening", "thinking", "speaking"].includes(voice.phase);
  useEffect(() => {
    if (!isTauri()) return;
    void api.getSettings().then((settings) => setPreferLocal(settings.preferLocalOnWake)).catch(() => {});
    void api.localPacks().then((packs) => setPacksReady(packsReadyForTalk(packs))).catch(() => {});
    let unlisten: (() => void) | undefined;
    void listen<Settings>("voice://settings", (event) => setPreferLocal(event.payload.preferLocalOnWake)).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);
  useEffect(() => {
    const el = draftBox.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
  }, [draft]);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [voice.rows, voice.busy, voice.sending]);
  useEffect(() => {
    sessionStorage.setItem("grok-talk-focus", focus ? "1" : "0");
  }, [focus]);
  useEffect(() => {
    localStorage.setItem("grok-mic-open", micOpen ? "1" : "0");
  }, [micOpen]);
  useEffect(() => {
    if (!focusHint) return;
    const id = window.setTimeout(() => setFocusHint(false), 2400);
    return () => window.clearTimeout(id);
  }, [focusHint]);
  useEffect(() => {
    if (!clearAsk) return;
    const id = window.setTimeout(() => document.getElementById("clear-talk-title")?.scrollIntoView({ block: "center" }), 200);
    return () => window.clearTimeout(id);
  }, [clearAsk]);
  useEffect(() => {
    if (!voice.lastError || !recoveryFor(voice.lastError)?.openAppSettings) return;
    const id = window.setTimeout(() => document.querySelector("[data-open-settings]")?.scrollIntoView({ block: "center" }), 200);
    return () => window.clearTimeout(id);
  }, [voice.lastError]);

  const onChatPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement | null;
    if (target?.closest("button, a, input, textarea, label, [role='button']")) return;
    const selection = window.getSelection();
    if (selection && !selection.isCollapsed) return;
    const now = Date.now();
    if (now - lastTap.current < 350) {
      lastTap.current = 0;
      if (!focus && sessionStorage.getItem("grok-focus-hint") !== "1") {
        sessionStorage.setItem("grok-focus-hint", "1");
        setFocusHint(true);
      }
      setFocus((on) => !on);
      return;
    }
    lastTap.current = now;
  };
  const onMic = () => {
    if (micOpen) {
      setMicOpen(false);
      return;
    }
    setMicOpen(true);
    if (!live) void voice.toggleSession();
  };
  const recovery = voice.lastError ? recoveryFor(voice.lastError) : null;
  const shownRows = voice.rows.length === 0 && previewQuery("bubbles") ? sampleBubbles() : voice.rows;
  const provider = readyProvider(voice.provider, accounts, preferLocal, packsReady);
  const inCall = voice.provider.length > 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className={cn("talk-head shrink-0 px-1", focus && "talk-head-off")}>
        <div>
          <div className="flex items-start justify-between gap-3 pb-3">
            <h1 className="masthead text-[1.85rem]">{t("Talk")}</h1>
            {clock ? <p className="stamp">{clock}</p> : null}
          </div>
          <div className="masthead-rule" />
          <div className="mt-3 flex flex-wrap items-center gap-2 pb-3">
            <p className="text-sm text-muted-foreground">
              {inCall ? `${t("Using")} ${t(provider)}` : accounts.length === 0 ? t("No voice yet") : t(provider)}
            </p>
            <button
              type="button"
              className={cn("stamp min-h-12 gap-2 px-3", voice.wakeEnabled && "stamp-on listen-live")}
              aria-pressed={voice.wakeEnabled}
              aria-label={voice.wakeEnabled ? t("Listen is on") : t("Listen is off")}
              onClick={() => {
                const next = !voice.wakeEnabled;
                void voice.toggleWake(next);
                if (next && localStorage.getItem("grok-listen-hint") !== "1") {
                  localStorage.setItem("grok-listen-hint", "1");
                  setListenHint(true);
                }
              }}
            >
              <Mic className="size-4" aria-hidden />
              {voice.wakeEnabled ? t("Listen · On") : t("Listen · Off")}
            </button>
            <button
              type="button"
              className="grid size-12 shrink-0 place-items-center text-muted-foreground"
              aria-expanded={aboutListen}
              aria-label={t("About Listen")}
              onClick={() => setAboutListen((on) => !on)}
            >
              <Info className="size-5" aria-hidden />
            </button>
          </div>
          {listenHint ? <p className="pb-3 text-sm leading-5 text-muted-foreground">{t("Stays on in the background until you turn it off.")}</p> : null}
          {aboutListen ? (
            <div className="mb-3 border border-border px-3 py-3 text-sm leading-5 text-muted-foreground">
              <p>{wakeTrained ? t("Stays on after you leave, lock the screen, or clear the app. Turn this off to stop.") : t("Hey Grok is not enrolled yet.")}</p>
              {!wakeTrained ? (
                <Button type="button" variant="outline" className="mt-3" onClick={() => onOpenSettings("wake")}>
                  {t("Set up Hey Grok")}
                </Button>
              ) : null}
            </div>
          ) : null}
        </div>
      </header>

      {voice.lastError && (
        <div data-open-settings={recovery?.openAppSettings ? "1" : undefined}>
        <Notice
          title={recovery?.openAppSettings ? t("Microphone is off") : t(voice.lastError)}
          body={recovery ? t(recovery.hint) : t("Try again. If it keeps happening, open Settings, then Debug.")}
          actionLabel={recovery?.openAppSettings ? t("Open settings") : recovery?.go ? t(settingsLabel(recovery.go)) : undefined}
          onAction={
            recovery?.openAppSettings
              ? () => {
                  if (isTauri()) void api.openAppSettings();
                }
              : recovery?.go
                ? () => onOpenSettings(recovery.go!)
                : undefined
          }
          onDismiss={voice.dismissError}
        />
        </div>
      )}

      <div ref={scroller} className="talk-log min-h-0 flex-1 overflow-y-auto px-1" onPointerUp={onChatPointerUp}>
        {shownRows.length === 0 ? (
          <div className="py-6">
            <p className="text-base leading-6 text-foreground">{t("Say “Hey Grok”, tap the microphone, or type when you can't.")}</p>
            <p className="mt-2 text-sm leading-6 text-muted-foreground">{t("Hearing, the model, the voice, and what is tried next.")}</p>
            <Button type="button" variant="outline" className="mt-4" onClick={() => onOpenSettings("voice")}>
              {t("Open voice and AI")}
            </Button>
          </div>
        ) : (
          <>
            <div className="flex justify-end">
              <Button size="sm" variant="ghost" onClick={() => setClearAsk(true)}>
                <Trash2 /> {t("Clear")}
              </Button>
            </div>
            {shownRows.map((r) => (
              <div key={`${r.role}-${r.id}`} className={cn("talk-bubble", r.role === "user" ? "talk-bubble-user" : "talk-bubble-grok")} aria-label={r.role === "user" ? t("you") : t("grok")}>
                {r.role === "user" || !r.text ? (
                  <p className="whitespace-pre-wrap">{r.text || "…"}</p>
                ) : (
                  <RichText text={r.text} />
                )}
              </div>
            ))}
          </>
        )}
        {(voice.sending || voice.busy) && (
          <div className="talk-bubble talk-bubble-grok" aria-label={t("grok")}>
            <p>{voice.busy ? t(voice.busy) : t("Searching the web…")}</p>
          </div>
        )}
      </div>

      {voice.handoff && (
        <div className="mag-panel mb-3">
          <p className="text-sm leading-6 text-foreground">{voice.handoff.summary}</p>
          <p className="mt-1 text-sm leading-5 text-muted-foreground">
            {voice.handoff.kind === "delete_person" || voice.handoff.kind === "cancel_scheduled" || voice.handoff.kind === "listen_off" || voice.handoff.kind === "wipe_memory"
              ? t("Say yes to finish this.")
              : t("Nothing is sent until you confirm in the other app.")}
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button type="button" onClick={voice.confirmHandoff}>
              {voice.handoff.kind === "delete_person" || voice.handoff.kind === "cancel_scheduled"
                ? t("Delete")
                : voice.handoff.kind === "wipe_memory"
                  ? t("Wipe memory")
                  : voice.handoff.kind === "listen_off"
                    ? t("Turn Listen off")
                    : t("Send the draft")}
            </Button>
            <Button type="button" variant="outline" onClick={voice.dismissHandoff}>
              {t("Not now")}
            </Button>
          </div>
        </div>
      )}

      {clearAsk ? (
        <div role="dialog" aria-modal="true" aria-labelledby="clear-talk-title" className="mag-panel mb-3">
          <p id="clear-talk-title" className="text-base leading-6 text-foreground">{t("Clear this talk?")}</p>
          <p className="mt-1 text-sm leading-5 text-muted-foreground">{t("This removes the conversation and the memory Talk keeps.")}</p>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              type="button"
              variant="destructive"
              onClick={() => {
                voice.clearTranscript();
                setClearAsk(false);
              }}
            >
              {t("Clear")}
            </Button>
            <Button type="button" variant="outline" onClick={() => setClearAsk(false)}>
              {t("Not now")}
            </Button>
          </div>
        </div>
      ) : null}

      <form
        className="talk-composer mt-auto flex shrink-0 items-end gap-2 border-t border-border pt-3"
        onSubmit={(e) => {
          e.preventDefault();
          const text = draft;
          setDraft("");
          void voice.sendText(text);
        }}
      >
        <textarea
          ref={draftBox}
          value={draft}
          rows={1}
          onChange={(e) => setDraft(e.target.value)}
          placeholder={t("Type a message")}
          aria-label={t("Type a message")}
          className="max-h-40 min-h-12 min-w-0 flex-1 resize-none overflow-y-auto border-0 border-b border-[var(--line)] bg-transparent px-0 py-3 text-base leading-6 text-foreground outline-none placeholder:text-muted-foreground focus:border-[var(--text-primary)]"
        />
        {draft.trim().length > 0 ? (
          <button
            type="submit"
            disabled={voice.sending}
            aria-label={t("Send")}
            className="grid size-12 shrink-0 place-items-center bg-foreground text-background disabled:opacity-40"
          >
            <Send className="size-5" strokeWidth={1.25} />
          </button>
        ) : (
          <button
            type="button"
            aria-pressed={micOpen}
            aria-label={micOpen ? t("Hide voice") : t("Show voice")}
            onClick={onMic}
            className={cn("grid size-12 shrink-0 place-items-center text-foreground", micOpen && "bg-foreground text-background")}
          >
            <Mic className="size-5" strokeWidth={1.25} />
          </button>
        )}
      </form>
      {live && !micOpen ? (
        <button type="button" className="mt-2 min-h-12 w-full text-left text-sm text-foreground" onClick={() => setMicOpen(true)}>
          {t(voice.phase === "speaking" ? "Speaking" : voice.phase === "thinking" ? "Thinking" : voice.phase === "connecting" ? "Connecting" : "Listening")}
        </button>
      ) : null}
      {focusHint ? <p className="pt-2 text-center text-sm text-muted-foreground">{t("Double-tap to show header")}</p> : null}
      <div className={cn("talk-mic-panel", micOpen && "open")}>
        <div>
          <button type="button" className="mx-auto mt-2 flex min-h-12 items-center gap-1 px-3 text-sm text-muted-foreground" aria-label={t("Hide voice")} onClick={() => setMicOpen(false)}>
            <ChevronDown className="size-4" aria-hidden />
            {t("Hide voice")}
          </button>
          <GlowRing phase={voice.phase} level={voice.level} onPress={() => void voice.toggleSession()} />
        </div>
      </div>
    </div>
  );
}
