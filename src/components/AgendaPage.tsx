import { useEffect, useState } from "react";
import { AppMark } from "@/components/AppMark";
import { CalendarPage } from "@/components/CalendarPage";
import { InfoTip } from "@/components/ds/InfoTip";
import { RemindersPage } from "@/components/RemindersPage";
import type { AgendaState } from "@/hooks/useAgenda";
import { useT } from "@/lib/i18n";
import { tabLabel, type TimeTab } from "@/lib/timeTabs";

type Pane = "calendar" | TimeTab;

const PANES: Pane[] = ["calendar", "reminders", "alarms", "countdowns"];

function paneFromQuery(): Pane {
  if (typeof window === "undefined") return "calendar";
  const asked = new URLSearchParams(window.location.search).get("tab");
  if (asked === "reminders" || asked === "alarms" || asked === "countdowns") return asked;
  return "calendar";
}

function paneLabel(pane: Pane): string {
  if (pane === "calendar") return "Calendar";
  return tabLabel(pane);
}

export function AgendaPage({ agenda, active = true }: { agenda: AgendaState; active?: boolean }) {
  const t = useT();
  const [pane, setPane] = useState<Pane>(paneFromQuery);
  useEffect(() => {
    const onPane = (event: Event) => {
      const next = (event as CustomEvent<string>).detail;
      if (next === "calendar" || next === "events") setPane("calendar");
      else if (next === "alarms" || next === "countdowns" || next === "reminders") setPane(next);
    };
    window.addEventListener("grok-agenda-pane", onPane);
    return () => window.removeEventListener("grok-agenda-pane", onPane);
  }, []);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="screen-head shrink-0">
        <div className="screen-head-row">
          <AppMark className="screen-mark" alt="" />
          <div className="title-with-info min-w-0 flex-1">
            <h1 data-fit="title" className="masthead text-foreground">{t("Agenda")}</h1>
            <InfoTip label={t("Agenda")} text={t("Calendar, reminders, alarms, and countdowns.")} />
          </div>
        </div>
        <div className="masthead-rule" />
      </header>
      <div role="tablist" aria-label={t("Agenda")} className="m3-tabs mb-3 shrink-0" style={{ gridTemplateColumns: `repeat(${PANES.length}, minmax(0, 1fr))` }}>
        {PANES.map((item) => (
          <button key={item} type="button" role="tab" id={`agenda-tab-${item}`} aria-selected={pane === item} aria-controls={`agenda-panel-${item}`} onClick={() => setPane(item)}>
            <span className="tab-label">{t(paneLabel(item))}</span>
          </button>
        ))}
      </div>
      <div id={`agenda-panel-${pane}`} role="tabpanel" aria-labelledby={`agenda-tab-${pane}`} className="flex min-h-0 flex-1 flex-col">
        {pane === "calendar" ? (
          <CalendarPage agenda={agenda} active={active} chrome={false} />
        ) : (
          <RemindersPage agenda={agenda} active={active} chrome={false} showTabs={false} forcedTab={pane} />
        )}
      </div>
    </div>
  );
}
