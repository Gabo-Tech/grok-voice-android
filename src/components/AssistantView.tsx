import { useEffect, useRef, useState, type ReactNode } from "react";
import { Trash2 } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { GlowRing } from "@/components/GlowRing";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import type { VoiceState } from "@/hooks/useVoice";
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
        <button
          key={i}
          type="button"
          className="break-all text-left underline"
          onClick={() => void openUrl(part)}
        >
          {part}
        </button>
      );
    }
    return <span key={i}>{part}</span>;
  });
}

export function AssistantView({ voice, wakeTrained }: { voice: VoiceState; wakeTrained: boolean }) {
  const scroller = useRef<HTMLDivElement>(null);
  const [draft, setDraft] = useState("");
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [voice.rows, voice.busy, voice.sending]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex shrink-0 items-center justify-between gap-3 px-1 pb-3">
        <div className="min-w-0">
          <h1 className="text-2xl leading-tight font-medium tracking-tight">Talk</h1>
          <p className="mt-1 text-xs leading-normal text-muted-foreground">
            {wakeTrained ? "Hands-free stays on after you leave" : "Enroll wake word in More"}
          </p>
        </div>
        <label className="flex items-center gap-2 text-[11px] tracking-[0.14em] text-muted-foreground uppercase">
          Listen
          <Switch checked={voice.wakeEnabled} onCheckedChange={(v) => void voice.toggleWake(v)} />
        </label>
      </header>

      {voice.lastError && (
        <p className="mb-3 shrink-0 border border-border px-3 py-2 text-xs text-foreground">{voice.lastError}</p>
      )}

      <div ref={scroller} className="min-h-0 flex-1 space-y-3 overflow-y-auto px-1">
        {voice.rows.length === 0 ? (
          <p className="py-8 text-sm leading-6 text-muted-foreground">
            Tap the square to talk, or type when you can't.
          </p>
        ) : (
          <>
            <div className="flex justify-end">
              <Button size="sm" variant="ghost" onClick={voice.clearTranscript}>
                <Trash2 /> Clear
              </Button>
            </div>
            {voice.rows.map((r) => (
              <div
                key={`${r.role}-${r.id}`}
                className={cn(
                  "max-w-[88%] px-0 py-1 text-base leading-6",
                  r.role === "user" ? "ml-auto text-right" : "mr-auto text-left",
                )}
              >
                <p className="mb-1 text-[10px] tracking-[0.18em] text-muted-foreground uppercase">
                  {r.role === "user" ? "you" : "grok"}
                </p>
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
            <p className="mb-1 text-[10px] tracking-[0.18em] text-muted-foreground uppercase">grok</p>
            <p className="text-muted-foreground">{voice.busy || "Searching the web…"}</p>
          </div>
        )}
      </div>

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
          placeholder="Type a question"
          enterKeyHint="send"
          className="min-w-0 flex-1 border border-border bg-transparent px-3 py-2 text-base text-foreground outline-none placeholder:text-muted-foreground"
        />
        <button
          type="submit"
          disabled={voice.sending || draft.trim().length === 0}
          className="shrink-0 border border-border px-3 py-2 text-xs tracking-[0.14em] text-foreground uppercase disabled:opacity-40"
        >
          {voice.sending ? "…" : "Send"}
        </button>
      </form>
      <div className="shrink-0 pt-4">
        <GlowRing phase={voice.phase} level={voice.level} onPress={() => void voice.toggleSession()} />
      </div>
    </div>
  );
}
