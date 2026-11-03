import { useEffect, useState } from "react";
import { Bug, ChevronRight, Wand2 } from "lucide-react";
import { DebugPanel } from "@/components/DebugPanel";
import { PageChrome, RowButton } from "@/components/PageChrome";
import { WakeView } from "@/components/WakeView";
import type { VoiceState } from "@/hooks/useVoice";
import { api, type Settings, type WakewordStatus } from "@/lib/api";
import { cn } from "@/lib/utils";

type Section = "menu" | "wake" | "debug";

export function MorePage({
  voice,
  wake,
  onWake,
  onSignedOut,
  onReplayTour,
}: {
  voice: VoiceState;
  wake: WakewordStatus;
  onWake: (s: WakewordStatus) => void;
  onSignedOut: () => void;
  onReplayTour: () => void;
}) {
  const [section, setSection] = useState<Section>("menu");
  const [quiet, setQuiet] = useState(10);

  useEffect(() => {
    void api.getSettings().then((s) => setQuiet(s.idleTimeoutSecs)).catch(() => {});
  }, []);

  const setQuietGap = (seconds: number) => {
    const next = Math.min(120, Math.max(5, seconds));
    setQuiet(next);
    void api.getSettings().then((current) => api.saveSettings({ ...current, idleTimeoutSecs: next } satisfies Settings));
  };

  if (section === "wake") {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <button
          type="button"
          className="mb-3 shrink-0 px-1 text-left text-xs leading-5 tracking-[0.16em] text-muted-foreground uppercase"
          onClick={() => setSection("menu")}
        >
          ← More
        </button>
        <WakeView status={wake} onStatus={onWake} />
      </div>
    );
  }

  if (section === "debug") {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <button
          type="button"
          className="mb-3 shrink-0 px-1 text-left text-xs leading-5 tracking-[0.16em] text-muted-foreground uppercase"
          onClick={() => setSection("menu")}
        >
          ← More
        </button>
        <DebugPanel voice={voice} onSignedOut={onSignedOut} onReplayTour={onReplayTour} />
      </div>
    );
  }

  return (
    <PageChrome title="More" subtitle="Wake word, quiet gap, diagnostics">
      <div className="mb-6 border-y border-border px-1 py-3">
        <div className="flex items-center justify-between gap-3">
          <div>
            <p className="text-[15px] text-foreground">Quiet gap</p>
            <p className="mt-0.5 text-xs text-muted-foreground">
              After this pause, the conversation ends and only the wake word is heard.
            </p>
          </div>
          <p className="shrink-0 text-sm tabular-nums text-foreground">{quiet}s</p>
        </div>
        <div className="mt-3 flex items-center gap-2">
          <button type="button" className="border border-border px-3 py-1 text-sm" onClick={() => setQuietGap(quiet - 5)}>
            −5
          </button>
          <input
            type="range"
            min={5}
            max={120}
            step={5}
            value={quiet}
            aria-label="Quiet gap in seconds"
            className="w-full accent-white"
            onChange={(e) => setQuietGap(Number(e.target.value))}
          />
          <button type="button" className="border border-border px-3 py-1 text-sm" onClick={() => setQuietGap(quiet + 5)}>
            +5
          </button>
        </div>
      </div>
      <div className="divide-y divide-border border-y border-border">
        <MenuRow
          icon={<Wand2 className="size-4" />}
          title="Wake word"
          detail={wake.trained ? "Hey Grok enrolled" : "Not enrolled yet"}
          onClick={() => setSection("wake")}
        />
        <MenuRow
          icon={<Bug className="size-4" />}
          title="Debug"
          detail="Logs, settings, sign out"
          onClick={() => setSection("debug")}
        />
      </div>
    </PageChrome>
  );
}

function MenuRow({
  icon,
  title,
  detail,
  onClick,
}: {
  icon: React.ReactNode;
  title: string;
  detail: string;
  onClick: () => void;
}) {
  return (
    <RowButton onClick={onClick} className="items-center">
      <span className={cn("grid size-9 place-items-center border border-border text-foreground")}>{icon}</span>
      <div className="min-w-0 flex-1">
        <p className="text-[15px] text-foreground">{title}</p>
        <p className="mt-0.5 text-xs text-muted-foreground">{detail}</p>
      </div>
      <ChevronRight className="size-4 text-muted-foreground" />
    </RowButton>
  );
}
