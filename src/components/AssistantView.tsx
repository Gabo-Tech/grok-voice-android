import { useEffect, useRef } from "react";
import { Ear, Trash2 } from "lucide-react";
import { GlowRing } from "@/components/GlowRing";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
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
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <Card className="shrink-0 flex-row items-center justify-between gap-3 px-4 py-3 shadow-none">
        <div className="flex items-center gap-3">
          <Ear className="size-4 text-primary" />
          <div>
            <Label htmlFor="wake-toggle" className="text-sm">
              Hands-free “Hey Grok”
            </Label>
            <p className="text-[11px] text-muted-foreground">
              {wakeTrained
                ? "Stays on after you leave. Stop it from the notification."
                : "Enroll your voice in the Wake tab"}
            </p>
          </div>
        </div>
        <Switch
          id="wake-toggle"
          checked={voice.wakeEnabled}
          onCheckedChange={(v) => void voice.toggleWake(v)}
        />
      </Card>

      {voice.lastError && (
        <p className="shrink-0 border border-border px-3 py-2 text-xs text-foreground">{voice.lastError}</p>
      )}

      <div ref={scroller} className="min-h-0 flex-1 space-y-2 overflow-y-auto border border-border bg-card p-3">
        <div className="flex items-center justify-between">
          <span className="text-[11px] tracking-widest text-muted-foreground uppercase">Chat</span>
          {voice.rows.length > 0 && (
            <Button size="sm" variant="ghost" onClick={voice.clearTranscript}>
              <Trash2 /> Clear
            </Button>
          )}
        </div>
        {voice.rows.length === 0 ? (
          <p className="py-8 text-center text-xs text-muted-foreground">What you say, and the reply, show up here.</p>
        ) : (
          voice.rows.map((r) => (
            <div
              key={`${r.role}-${r.id}`}
              className={cn(
                "max-w-[85%] border px-3 py-2 text-sm",
                r.role === "user" ? "ml-auto border-foreground bg-foreground text-background" : "mr-auto border-border bg-background",
              )}
            >
              <Badge
                variant="outline"
                className={cn(
                  "mb-1 text-[10px] uppercase",
                  r.role === "user" && "border-background text-background",
                )}
              >
                {r.role === "user" ? "you" : "grok"}
              </Badge>
              <p className="whitespace-pre-wrap">{typeof r.text === "string" && r.text.length > 0 ? r.text : "…"}</p>
            </div>
          ))
        )}
      </div>

      <GlowRing phase={voice.phase} level={voice.level} onPress={() => void voice.toggleSession()} />
    </div>
  );
}
