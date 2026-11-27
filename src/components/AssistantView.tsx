import { useEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Info, Keyboard, Mic, Send, Trash2 } from "lucide-react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { InfoTip } from "@/components/ds/InfoTip";
import { GlowRing } from "@/components/GlowRing";
import { Presence, presencePose, previewPhase } from "@/components/Presence";
import { Notice } from "@/components/PageChrome";
import { Sheet } from "@/components/Sheet";
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
  const [typing, setTyping] = useState(() => previewQuery("keyboard"));
  const [aboutListen, setAboutListen] = useState(false);
  const [statusOpen, setStatusOpen] = useState(false);
  const [listenHint, setListenHint] = useState(false);
  const [focusHint, setFocusHint] = useState(false);
  const [clearAsk, setClearAsk] = useState(() => previewQuery("clear"));
  const lastTap = useRef(0);
  const clock = sessionClock(voice.usage?.openSinceMs);
  const shownPhase = previewPhase() ?? voice.phase;
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
    const fit = () => {
      el.style.height = "auto";
      el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
    };
    fit();
    const soon = window.setTimeout(fit, 160);
    const later = window.setTimeout(fit, 480);
    return () => {
      window.clearTimeout(soon);
      window.clearTimeout(later);
    };
  }, [draft]);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const pin = () => {
      if (voice.rows.length === 0) {
        el.scrollTop = 0;
        return;
      }
      el.scrollTop = el.scrollHeight;
    };
    pin();
    const soon = window.setTimeout(pin, 160);
    const later = window.setTimeout(pin, 480);
    return () => {
      window.clearTimeout(soon);
      window.clearTimeout(later);
    };
  }, [voice.rows, voice.busy, voice.sending, voice.lastError]);
  useEffect(() => {
    sessionStorage.setItem("grok-talk-focus", focus ? "1" : "0");
  }, [focus]);
  useEffect(() => {
    if (!focusHint) return;
    const id = window.setTimeout(() => setFocusHint(false), 2400);
    return () => window.clearTimeout(id);
  }, [focusHint]);
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
            <span className="inline-flex items-center">
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
            </span>
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

      <div ref={scroller} className="talk-log min-h-0 flex-1 overflow-y-auto px-1" onPointerUp={onChatPointerUp}>
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
        {shownRows.length === 0 ? (
          <div className="talk-empty">
            <Presence phase={shownPhase} level={voice.level} onOpen={() => setStatusOpen(true)} />
            <div className="title-line">
              <p className="text-base text-foreground">{t("What should we do?")}</p>
              <InfoTip label={t("Talk")}>
                <p>{t("Say “Hey Grok”, tap the microphone, or type when you can't.")}</p>
                <p>{t("Hearing, the model, the voice, and what is tried next.")}</p>
                <p>{t("Open voice and AI")}</p>
              </InfoTip>
            </div>
            <div className="flex max-w-full flex-wrap justify-center gap-2">
              {["What's on today?", "Add reminder", "How much did I spend?"].map((line) => (
                <button key={line} type="button" className="period-chip" onClick={() => void voice.sendText(t(line))}>
                  {t(line)}
                </button>
              ))}
            </div>
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
        {(voice.sending || voice.busy || presencePose(shownPhase) === "thinking") && (
          <div className="talk-bubble talk-bubble-grok" aria-label={t("grok")}>
            <p>
              {voice.busy ? t(voice.busy) : voice.sending ? t("Searching the web…") : t(shownPhase === "connecting" ? "Connecting" : "Thinking")}
            </p>
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
            <Button
              type="button"
              variant={voice.handoff.kind === "delete_person" || voice.handoff.kind === "cancel_scheduled" || voice.handoff.kind === "wipe_memory" ? "destructive" : "default"}
              onClick={voice.confirmHandoff}
            >
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

      {clearAsk
        ? createPortal(
            <Sheet
              open
              title={t("Clear this talk?")}
              onClose={() => setClearAsk(false)}
              footer={
                <div className="flex flex-col gap-2">
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
              }
            >
              <p className="text-base leading-6 text-foreground">{t("This removes the conversation and the memory Talk keeps.")}</p>
            </Sheet>,
            document.body,
          )
        : null}

      {typing ? (
        <form
          className="talk-composer mt-auto flex shrink-0 items-end gap-2 border-t border-border pt-3"
          onSubmit={(e) => {
            e.preventDefault();
            const text = draft;
            setDraft("");
            void voice.sendText(text);
          }}
        >
          <button type="button" className="icon-hit" aria-label={t("Start voice session")} onClick={() => setTyping(false)}>
            <Mic className="size-5" strokeWidth={1.5} />
          </button>
          <textarea
            ref={draftBox}
            value={draft}
            rows={1}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={t("Type a message")}
            aria-label={t("Type a message")}
            className="max-h-40 min-h-12 min-w-0 flex-1 resize-none overflow-y-auto border-0 border-b border-[var(--line)] bg-transparent px-0 py-3 text-base leading-6 text-foreground outline-none placeholder:text-muted-foreground focus:border-[var(--text-primary)]"
          />
          <button
            type="submit"
            disabled={voice.sending || draft.trim().length === 0}
            aria-label={t("Send")}
            className="grid size-12 shrink-0 place-items-center bg-foreground text-background disabled:opacity-40"
          >
            <Send className="size-5" strokeWidth={1.5} />
          </button>
        </form>
      ) : (
        <div className="talk-dock mt-auto shrink-0">
          <button type="button" className="icon-hit" aria-label={t("Type a message")} onClick={() => setTyping(true)}>
            <Keyboard className="size-5" strokeWidth={1.5} />
          </button>
          <GlowRing phase={shownPhase} level={voice.level} onPress={() => void voice.toggleSession()} />
          {shownRows.length === 0 ? (
            <span className="icon-hit" aria-hidden />
          ) : (
            <Presence phase={shownPhase} level={voice.level} onOpen={() => setStatusOpen(true)} />
          )}
        </div>
      )}
      {focusHint ? <p className="pt-2 text-center text-sm text-muted-foreground">{t("Double-tap to show header")}</p> : null}
      <Sheet open={statusOpen} title={t("Voice status")} onClose={() => setStatusOpen(false)}>
        <p className="text-base text-foreground">
          {t(shownPhase === "speaking" ? "Speaking" : shownPhase === "thinking" ? "Thinking" : shownPhase === "connecting" ? "Connecting" : shownPhase === "listening" ? "Listening" : shownPhase === "recording" ? "Recording" : shownPhase === "wake" ? "Waiting for Hey Grok" : "Ready")}
        </p>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          {inCall ? `${t("Using")} ${t(provider)}` : accounts.length === 0 ? t("No voice yet") : t(provider)}
        </p>
      </Sheet>
    </div>
  );
}
