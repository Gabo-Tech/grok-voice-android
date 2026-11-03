import { useState } from "react";
import { Bug, ChevronRight, Wand2 } from "lucide-react";
import { DebugPanel } from "@/components/DebugPanel";
import { PageChrome, RowButton } from "@/components/PageChrome";
import { WakeView } from "@/components/WakeView";
import type { VoiceState } from "@/hooks/useVoice";
import type { WakewordStatus } from "@/lib/api";
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
    <PageChrome title="More" subtitle="Wake word and diagnostics">
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
