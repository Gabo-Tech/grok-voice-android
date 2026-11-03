import { useEffect, useRef } from "react";
import { Trash2 } from "lucide-react";
import { GlowRing } from "@/components/GlowRing";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import type { VoiceState } from "@/hooks/useVoice";
import { cn } from "@/lib/utils";

export function AssistantView({ voice, wakeTrained }: { voice: VoiceState; wakeTrained: boolean }) {
  const scroller = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [voice.rows]);

  return (
    <div className="flex min-h-0 flex-1 flex-col animate-page-in">
      <header className="flex shrink-0 items-center justify-between gap-3 px-1 pb-3">
        <div>
          <h1 className="text-2xl font-medium tracking-tight">Talk</h1>
          <p className="mt-1 text-xs text-muted-foreground">
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
          <div className="grid h-full min-h-[12rem] place-items-center text-center">
            <p className="max-w-[14rem] text-xs leading-relaxed text-muted-foreground">
              Tap the square to talk. Transcripts land here, quietly.
            </p>
          </div>
        ) : (
          <>
            <div className="flex justify-end">
              <Button size="sm" variant="ghost" onClick={voice.clearTranscript}>
                <Trash2 /> Clear
              </Button>
            </div>
            {voice.rows.map((r, i) => (
              <div
                key={`${r.role}-${r.id}`}
                className={cn(
                  "max-w-[88%] animate-row-in px-0 py-1 text-sm leading-relaxed",
                  r.role === "user" ? "ml-auto text-right" : "mr-auto text-left",
                )}
                style={{ animationDelay: `${Math.min(i, 6) * 30}ms` }}
              >
                <p className="mb-1 text-[10px] tracking-[0.18em] text-muted-foreground uppercase">
                  {r.role === "user" ? "you" : "grok"}
                </p>
                <p
                  className={cn(
                    "whitespace-pre-wrap",
                    r.role === "user" ? "text-foreground" : "text-foreground/90",
                  )}
                >
                  {typeof r.text === "string" && r.text.length > 0 ? r.text : "…"}
                </p>
              </div>
            ))}
          </>
        )}
      </div>

      <div className="shrink-0 border-t border-border pt-4">
        <GlowRing phase={voice.phase} level={voice.level} onPress={() => void voice.toggleSession()} />
      </div>
    </div>
  );
}
