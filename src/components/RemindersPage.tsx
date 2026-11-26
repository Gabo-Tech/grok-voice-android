import { useEffect, useMemo, useRef, useState } from "react";
import { Trash2 } from "lucide-react";
import { EmptyState, Notice, PageChrome } from "@/components/PageChrome";
import { ShareButton } from "@/components/ShareButton";
import { Sheet } from "@/components/Sheet";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import type { AgendaState } from "@/hooks/useAgenda";
import { formatClock, fromLocalInput, toLocalInput } from "@/hooks/useAgenda";
import { RepeatFields, endOfDay, untilInput, type RepeatChoice, type RepeatUnit } from "@/components/RepeatFields";
import { AlarmSoundFields, type SoundMode } from "@/components/AlarmSoundFields";
import { api, type ScheduledItem } from "@/lib/api";
import { fill, useT } from "@/lib/i18n";
import {
  TIME_TABS,
  addLabel,
  groupTimeItems,
  isRepeating,
  kindForTab,
  previewTimeItems,
  queryFor,
  recurrenceValue,
  repeatFrom,
  sectionKey,
  tabForKind,
  tabLabel,
  type TimeTab,
} from "@/lib/timeTabs";

const KINDS = ["reminder", "alarm", "timer"] as const;
const WEEKDAYS = [
  { id: 1, label: "Mon" },
  { id: 2, label: "Tue" },
  { id: 3, label: "Wed" },
  { id: 4, label: "Thu" },
  { id: 5, label: "Fri" },
  { id: 6, label: "Sat" },
  { id: 7, label: "Sun" },
] as const;

type Repeat = RepeatChoice | "once";
type Delivery = "" | "spoken" | "normal";

export function RemindersPage({ agenda, active = true }: { agenda: AgendaState; active?: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<ScheduledItem | null>(null);
  const [title, setTitle] = useState("");
  const [at, setAt] = useState(() => toLocalInput(Date.now() + 3_600_000));
  const [minutes, setMinutes] = useState(10);
  const [note, setNote] = useState("");
  const [kind, setKind] = useState<(typeof KINDS)[number]>("reminder");
  const [tab, setTab] = useState<TimeTab>("reminders");
  const [queries, setQueries] = useState<Record<TimeTab, string>>({ reminders: "", alarms: "", countdowns: "" });
  const [repeat, setRepeat] = useState<Repeat>("none");
  const [interval, setInterval] = useState(2);
  const [unit, setUnit] = useState<RepeatUnit>("days");
  const [weekdays, setWeekdays] = useState<number[]>([]);
  const [clock, setClock] = useState("09:00");
  const [date, setDate] = useState("");
  const [leadDays, setLeadDays] = useState(0);
  const [personId, setPersonId] = useState("");
  const [until, setUntil] = useState("");
  const [endCount, setEndCount] = useState("");
  const [delivery, setDelivery] = useState<Delivery>("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [soundMode, setSoundMode] = useState<SoundMode>("default");
  const [soundFileRef, setSoundFileRef] = useState("");
  const [soundFileName, setSoundFileName] = useState("");
  const [soundUrl, setSoundUrl] = useState("");
  const [soundError, setSoundError] = useState<string | null>(null);
  const openNewRef = useRef<(next: (typeof KINDS)[number]) => void>(() => {});

  const previewing =
    import.meta.env.DEV && typeof window !== "undefined" && new URLSearchParams(window.location.search).get("timePreview") === "1";
  const stored = agenda.agenda.reminders;
  const items: ScheduledItem[] =
    stored.length > 0 || !previewing
      ? stored
      : previewTimeItems().map((item) => ({
          ...item,
          recurrence: item.id === 1 ? "every_n_days" : "",
          interval: item.id === 1 ? 2 : 1,
          endCount: item.id === 1 ? 5 : 0,
          delivery: item.id === 2 ? "spoken" : "",
          weekdays: [],
          time: "09:00",
          date: "",
          leadDays: 0,
          untilMs: 0,
        }));

  const openNew = (next: (typeof KINDS)[number] = "reminder") => {
    setEditing(null);
    setTitle("");
    setKind(next);
    setTab(tabForKind(next));
    setAt(toLocalInput(Date.now() + 3_600_000));
    setMinutes(next === "timer" ? 10 : 60);
    setNote("");
    setRepeat("none");
    setInterval(2);
    setUnit("days");
    setWeekdays([]);
    setClock("09:00");
    setDate("");
    setLeadDays(0);
    setPersonId("");
    setUntil("");
    setEndCount("");
    setDelivery("");
    setConfirmDelete(false);
    setSoundMode("default");
    setSoundFileRef("");
    setSoundFileName("");
    setSoundUrl("");
    setSoundError(null);
    setOpen(true);
  };
  openNewRef.current = openNew;

  useEffect(() => {
    if (!import.meta.env.DEV || typeof window === "undefined") return;
    const params = new URLSearchParams(window.location.search);
    const clockTab = params.get("clock");
    if (clockTab === "reminders" || clockTab === "alarms" || clockTab === "countdowns") setTab(clockTab);
    const sheet = params.get("sheet");
    if (sheet === "reminder" || sheet === "alarm" || sheet === "countdown") {
      openNewRef.current(sheet === "alarm" ? "alarm" : sheet === "countdown" ? "timer" : "reminder");
    }
  }, []);

  const openEdit = (item: ScheduledItem) => {
    setEditing(item);
    setTitle(item.title);
    const nextKind = KINDS.includes(item.kind as (typeof KINDS)[number]) ? (item.kind as (typeof KINDS)[number]) : "reminder";
    setKind(item.kind === "countdown" ? "timer" : nextKind);
    setAt(toLocalInput(item.atMs));
    setMinutes(Math.max(1, Math.round((item.atMs - Date.now()) / 60_000)));
    setNote(item.note || "");
    const parsed = repeatFrom(item.recurrence, item.interval);
    setRepeat(parsed.repeat as Repeat);
    setUnit(parsed.unit);
    setInterval(parsed.interval);
    setWeekdays(item.weekdays ?? []);
    setClock(item.time || clockFrom(item.atMs));
    setDate(item.date || "");
    setLeadDays(item.leadDays ?? 0);
    setPersonId(item.personId || "");
    setUntil(untilInput(item.untilMs));
    setEndCount(item.endCount ? String(item.endCount) : "");
    setDelivery(item.delivery === "spoken" || item.delivery === "normal" ? item.delivery : "");
    setConfirmDelete(false);
    const nextSound = item.soundKind === "file" || item.soundKind === "youtube" ? item.soundKind : "default";
    setSoundMode(nextSound);
    setSoundFileRef(item.soundKind === "file" ? item.soundRef || "" : "");
    setSoundFileName(item.soundKind === "file" ? item.soundLabel || item.soundRef || "" : "");
    setSoundUrl(item.soundKind === "youtube" ? item.soundLabel || item.soundRef || "" : "");
    setSoundError(null);
    setOpen(true);
  };

  const close = () => {
    setOpen(false);
    setEditing(null);
    setConfirmDelete(false);
  };

  const pattern = recurrenceValue(repeat === "once" ? "once" : repeat, unit);
  const linked = Boolean(personId);
  const repeating = (kind === "reminder" || kind === "alarm") && (isRepeating(pattern) || linked);
  const whenMs = () => (kind === "timer" ? Date.now() + Math.max(1, minutes) * 60_000 : fromLocalInput(at));

  const save = () => {
    const next = title.trim();
    if (!next) return;
    if (kind === "alarm" && soundMode === "file" && !soundFileRef) {
      setSoundError("Choose an audio file.");
      return;
    }
    if (kind === "alarm" && soundMode === "youtube" && !soundUrl.trim()) {
      setSoundError("That is not a YouTube video link.");
      return;
    }
    setSoundError(null);
    const customSound = kind === "alarm" && soundMode !== "default";
    const count = endCount ? Math.max(1, Number(endCount) || 1) : 0;
    void agenda.run(async () => {
      const snap = await api.saveScheduled({
        title: next,
        atMs: repeating ? Date.now() + 120_000 : whenMs(),
        note: note.trim(),
        kind,
        id: editing?.id,
        recurrence: kind === "timer" ? "" : pattern,
        weekdays: repeat === "weekly" || (repeat === "custom" && unit === "weeks") ? weekdays : [],
        time: repeating ? clock : "",
        date: linked || repeat === "yearly" || repeat === "once" || repeat === "monthly" || (repeat === "custom" && (unit === "months" || unit === "years")) ? date : "",
        leadDays: linked || repeat === "yearly" || (repeat === "custom" && unit === "years") ? leadDays : 0,
        untilMs: repeating && until && !count ? endOfDay(until) : 0,
        interval: repeat === "custom" ? Math.max(1, interval) : 1,
        endCount: repeating ? count : 0,
        delivery,
        soundKind: customSound ? soundMode : "",
        soundRef: customSound ? (soundMode === "file" ? soundFileRef : soundUrl.trim()) : "",
        soundLabel: customSound ? (soundMode === "file" ? soundFileName : soundUrl.trim()) : "",
      });
      close();
      return snap;
    });
  };

  const remove = (id: number) => {
    if (!confirmDelete) {
      setConfirmDelete(true);
      return;
    }
    void agenda.run(async () => {
      const snap = await api.cancelScheduled(id);
      if (editing?.id === id) close();
      return snap;
    });
  };

  const finish = (id: number) => {
    void agenda.run(async () => api.completeScheduled(id));
  };

  const soundProblem =
    soundError ||
    (kind === "alarm" && agenda.error && SOUND_ERRORS.some((item) => agenda.error?.includes(item)) ? agenda.error : null);
  const query = queryFor(queries, tab);
  const grouped = useMemo(() => groupTimeItems(items, tab, query), [items, query, tab]);
  const subtitle = waitingLine(grouped, t);
  const setQuery = (value: string) => setQueries((current) => ({ ...current, [tab]: value }));

  return (
    <>
      <PageChrome title={t("Time")} subtitle={subtitle} onAdd={() => openNew(kindForTab(tab))} addLabel={t(addLabel(tab))} active={active}>
        <div role="tablist" aria-label={t("Time")} className="mb-4 flex flex-wrap gap-x-4 gap-y-1">
          {TIME_TABS.map((item) => (
            <button
              key={item}
              type="button"
              role="tab"
              id={`time-tab-${item}`}
              aria-selected={tab === item}
              aria-controls={`time-panel-${item}`}
              onClick={() => setTab(item)}
              className={
                tab === item
                  ? "min-h-11 px-0 text-sm text-foreground underline decoration-[var(--brand)] underline-offset-4"
                  : "min-h-11 px-0 text-sm text-muted-foreground"
              }
            >
              {t(tabLabel(item))}
            </button>
          ))}
        </div>
        <div key={tab} role="tabpanel" id={`time-panel-${tab}`} aria-labelledby={`time-tab-${tab}`} data-time-tabpanel={tab}>
          <button
            type="button"
            className="mb-4 flex h-12 w-full items-center justify-center rounded-[var(--radius)] bg-[var(--text-primary)] px-4 text-base text-[var(--surface-0)]"
            onClick={() => openNew(kindForTab(tab))}
          >
            {t(addLabel(tab))}
          </button>
          {(items.length > 0 || query) && (
            <div className="mb-6">
              <Field label={t("Search")}>
                <div className="relative">
                  <Input
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    placeholder={t("A title or a note")}
                    aria-label={t("Search reminders")}
                    className="pr-16"
                  />
                  {query && (
                    <button type="button" className="absolute top-0 right-0 h-12 px-3 text-sm text-foreground" onClick={() => setQuery("")}>
                      {t("Clear")}
                    </button>
                  )}
                </div>
              </Field>
            </div>
          )}
          {agenda.error && !open && <Notice title={t(agenda.error)} />}
          {grouped.total === 0 ? (
            <EmptyState
              title={query ? t("No matches") : t("Nothing scheduled")}
              body={query ? t("Try another word.") : t(emptyBody(tab))}
              actionLabel={query ? t("Clear search") : t(addLabel(tab))}
              onAction={query ? () => setQuery("") : () => openNew(kindForTab(tab))}
            />
          ) : (
            <div className="space-y-6">
              {grouped.sections.map((section) => (
                <section key={sectionKey(tab, section.name)}>
                  <h2 className="mb-2 px-1 text-[13px] text-muted-foreground">{t(section.name)}</h2>
                  <div className="divide-y divide-border border-y border-border">
                    {section.items.map((item) => (
                      <div key={`${tab}-${item.id}`} className="flex items-start gap-3 px-1 py-3.5">
                        <button
                          type="button"
                          aria-label={fill("Mark {title} done", { title: item.title })}
                          className="grid size-12 shrink-0 place-items-center"
                          onClick={() => finish(item.id)}
                        >
                          <span className="size-5 rounded-full border border-foreground" />
                        </button>
                        <button type="button" className="flex min-w-0 flex-1 items-start gap-4 text-left" onClick={() => openEdit(item)}>
                          <div className="w-16 shrink-0 pt-0.5 text-right">
                            <p className="text-sm leading-5 tabular-nums text-foreground">{formatClock(item.atMs)}</p>
                            {section.name === "Later" && (
                              <p className="text-[13px] leading-4 text-muted-foreground">
                                {new Date(item.atMs).toLocaleDateString(undefined, { month: "short", day: "numeric" })}
                              </p>
                            )}
                          </div>
                          <div className="min-w-0 flex-1 border-l border-border pl-4">
                            <p className="text-[13px] text-muted-foreground">
                              {[
                                kindName(item.kind, t),
                                patternLabel(item, t),
                                item.delivery === "spoken" ? t("Spoken alert") : "",
                                item.soundKind === "file" || item.soundKind === "youtube" ? t("Custom sound") : "",
                              ]
                                .filter(Boolean)
                                .join(" · ")}
                            </p>
                            <p className="truncate text-base leading-6 text-foreground">{item.title}</p>
                            {item.note ? <p className="mt-1 truncate text-sm leading-5 text-muted-foreground">{item.note}</p> : null}
                          </div>
                        </button>
                      </div>
                    ))}
                  </div>
                </section>
              ))}
            </div>
          )}
        </div>
      </PageChrome>

      <Sheet
        open={open}
        title={editing ? t(editKindLabel(kind)) : t(newKindLabel(kind))}
        onClose={close}
        footer={
          <div className="flex flex-wrap items-center gap-2">
            <ShareButton
              title={title}
              text={[kindName(kind, t), repeating ? `${clock} ${date}`.trim() : kind === "timer" ? fill("{n} min", { n: minutes }) : at, note]
                .filter(Boolean)
                .join("\n")}
            />
            {editing && (
              <Button variant="ghost" disabled={agenda.busy} onClick={() => remove(editing.id)}>
                <Trash2 /> {confirmDelete ? t("Delete") : t("Delete")}
              </Button>
            )}
            <div className="flex-1" />
            <Button variant="ghost" onClick={close}>
              {t("Cancel")}
            </Button>
            <Button disabled={agenda.busy || !title.trim()} onClick={save}>
              {t("Save")}
            </Button>
          </div>
        }
      >
        <div className="space-y-5">
          {confirmDelete && editing && (
            <div>
              <p className="text-base leading-6 text-foreground">{t(deleteAsk(kind))}</p>
              <p className="mt-1 text-sm leading-5 text-muted-foreground">{t("Tap Delete again.")}</p>
            </div>
          )}
          <div className="grid grid-cols-3 gap-2">
            {KINDS.map((item) => (
              <button
                key={item}
                type="button"
                onClick={() => setKind(item)}
                className={
                  item === kind
                    ? "flex min-h-12 items-center justify-center bg-[var(--text-primary)] px-1 text-center text-sm leading-tight text-[var(--surface-0)]"
                    : "flex min-h-12 items-center justify-center px-1 text-center text-sm leading-tight text-muted-foreground"
                }
              >
                {kindName(item, t)}
              </button>
            ))}
          </div>
          <Field label={t("Title")} error={open && agenda.error && !soundProblem ? t(agenda.error) : null}>
            <Input autoFocus value={title} aria-invalid={Boolean(agenda.error) || undefined} onChange={(e) => setTitle(e.target.value)} placeholder={t("Call the clinic")} />
          </Field>
          {(kind === "reminder" || kind === "alarm") && !linked && (
            <RepeatFields
              repeat={repeat === "once" ? "none" : repeat}
              weekdays={weekdays}
              until={until}
              interval={interval}
              unit={unit}
              endCount={endCount}
              onRepeat={setRepeat}
              onWeekdays={setWeekdays}
              onUntil={setUntil}
              onInterval={setInterval}
              onUnit={setUnit}
              onEndCount={setEndCount}
            />
          )}
          {kind === "reminder" && linked && (
            <div>
              <p className="mb-2 text-sm text-muted-foreground">{t("Repeats")}</p>
              <div className="grid grid-cols-2 gap-2">
                {(["yearly", "once"] as const).map((item) => (
                  <button
                    key={item}
                    type="button"
                    aria-pressed={repeat === item}
                    onClick={() => setRepeat(item)}
                    className={
                      repeat === item
                        ? "h-14 bg-[var(--text-primary)] text-sm font-medium text-[var(--surface-0)]"
                        : "h-14 text-sm text-muted-foreground"
                    }
                  >
                    {item === "once" ? t("Just once") : t("Every year")}
                  </button>
                ))}
              </div>
              <p className="mt-2 text-sm leading-5 text-muted-foreground">{t("This date belongs to someone in People.")}</p>
            </div>
          )}
          {kind === "timer" ? (
            <Field label={t("Length")} hint={t("Counts from the moment you save.")}>
              <div className="mb-3 flex gap-2">
                {[5, 10, 25].map((value) => (
                  <button
                    key={value}
                    type="button"
                    onClick={() => setMinutes(value)}
                    className={
                      minutes === value
                        ? "h-12 flex-1 bg-[var(--text-primary)] text-sm text-[var(--surface-0)]"
                        : "h-12 flex-1 text-sm text-muted-foreground"
                    }
                  >
                    {fill("{n} min", { n: value })}
                  </button>
                ))}
              </div>
              <Input type="number" min={1} inputMode="numeric" value={minutes} onChange={(e) => setMinutes(Math.max(1, Number(e.target.value) || 1))} />
            </Field>
          ) : repeating ? (
            <div className="space-y-4">
              {(linked || repeat === "yearly" || repeat === "once" || repeat === "monthly" || (repeat === "custom" && (unit === "months" || unit === "years"))) && (
                <Field label={t("Date")}>
                  <Input type="date" aria-label={t("Date")} value={asDateInput(date)} onChange={(e) => setDate(e.target.value)} />
                </Field>
              )}
              <Field label={t("Time")} hint={t("Next notification uses this clock.")}>
                <Input type="time" value={clock} onChange={(e) => setClock(e.target.value)} />
              </Field>
              {(linked || repeat === "yearly" || (repeat === "custom" && unit === "years")) && (
                <Field label={t("Early")}>
                  <div className="flex gap-2">
                    {[0, 1, 3, 7].map((days) => (
                      <button
                        key={days}
                        type="button"
                        onClick={() => setLeadDays(days)}
                        className={
                          leadDays === days
                            ? "h-12 flex-1 bg-[var(--text-primary)] text-sm text-[var(--surface-0)]"
                            : "h-12 flex-1 text-sm text-muted-foreground"
                        }
                      >
                        {days === 0 ? t("Day of") : fill("{n}d", { n: days })}
                      </button>
                    ))}
                  </div>
                </Field>
              )}
            </div>
          ) : (
            <Field label={t("When")}>
              <div className="mb-3 flex flex-col gap-2">
                {quickTimes(t).map((choice) => (
                  <button
                    key={choice.label}
                    type="button"
                    onClick={() => setAt(toLocalInput(choice.ms))}
                    className={
                      at === toLocalInput(choice.ms)
                        ? "h-12 bg-[var(--text-primary)] px-3 text-left text-sm text-[var(--surface-0)]"
                        : "h-12 px-3 text-left text-sm text-muted-foreground"
                    }
                  >
                    {choice.label}
                  </button>
                ))}
              </div>
              <Input type="datetime-local" value={at} onChange={(e) => setAt(e.target.value)} />
            </Field>
          )}
          <DeliveryFields value={delivery} onChange={setDelivery} />
          {kind === "alarm" && (
            <AlarmSoundFields
              mode={soundMode}
              fileName={soundFileName}
              fileRef={soundFileRef}
              url={soundUrl}
              error={soundProblem}
              busy={agenda.busy}
              onMode={setSoundMode}
              onFile={(relativePath, name) => {
                setSoundFileRef(relativePath);
                setSoundFileName(name);
              }}
              onUrl={setSoundUrl}
              onError={setSoundError}
            />
          )}
          <Field label={t("Note")}>
            <Textarea value={note} onChange={(e) => setNote(e.target.value)} placeholder={t("Bring the referral")} className="min-h-28" />
          </Field>
        </div>
      </Sheet>
    </>
  );
}

function DeliveryFields({ value, onChange }: { value: Delivery; onChange: (value: Delivery) => void }) {
  const t = useT();
  const choices: Array<{ id: Delivery; label: string }> = [
    { id: "", label: "Same as Settings" },
    { id: "spoken", label: "Speak this one" },
    { id: "normal", label: "Stay quiet" },
  ];
  return (
    <div>
      <p className="mb-2 text-sm text-muted-foreground" id="spoken-alert-label">
        {t("Spoken alert")}
      </p>
      <div className="grid grid-cols-1 gap-2" role="group" aria-labelledby="spoken-alert-label">
        {choices.map((choice) => (
          <button
            key={choice.label}
            type="button"
            aria-pressed={value === choice.id}
            onClick={() => onChange(choice.id)}
            className={
              value === choice.id
                ? "h-12 bg-[var(--text-primary)] px-3 text-left text-sm text-[var(--surface-0)]"
                : "h-12 px-3 text-left text-sm text-muted-foreground"
            }
          >
            {t(choice.label)}
          </button>
        ))}
      </div>
    </div>
  );
}

const SOUND_ERRORS = [
  "That is not a YouTube video link.",
  "That audio file is missing. Pick it again.",
  "That audio file is too large.",
  "Choose an audio file.",
  "No file was chosen.",
  "Could not read that audio file.",
  "Custom sound is only for alarms.",
  "Could not open the file picker.",
  "That audio file is not saved on this phone. Pick it in the alarm form.",
  "Choose either a YouTube link or an audio file.",
  "An alarm is already playing.",
  "Choosing an audio file works on the Android phone.",
];

function kindName(kind: string, t: (english: string) => string) {
  if (kind === "alarm") return t("Alarm");
  if (kind === "timer" || kind === "countdown") return t("Countdown");
  return t("Reminder");
}

function newKindLabel(kind: string) {
  if (kind === "alarm") return "New alarm";
  if (kind === "timer") return "New countdown";
  return "New reminder";
}

function editKindLabel(kind: string) {
  if (kind === "alarm") return "Edit alarm";
  if (kind === "timer") return "Edit countdown";
  return "Edit reminder";
}

function deleteAsk(kind: string) {
  if (kind === "alarm") return "Delete this alarm?";
  if (kind === "timer" || kind === "countdown") return "Delete this countdown?";
  return "Delete this reminder?";
}

function emptyBody(tab: TimeTab) {
  if (tab === "alarms") return "Add an alarm.";
  if (tab === "countdowns") return "Add a countdown.";
  return "Add a reminder.";
}

function quickTimes(t: (english: string) => string): Array<{ label: string; ms: number }> {
  const now = new Date();
  const tonight = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 20, 0, 0, 0);
  if (tonight.getTime() <= now.getTime()) tonight.setDate(tonight.getDate() + 1);
  const morning = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 8, 0, 0, 0);
  return [
    { label: t("In 1 hour"), ms: now.getTime() + 3_600_000 },
    { label: t("Tonight"), ms: tonight.getTime() },
    { label: t("Tomorrow morning"), ms: morning.getTime() },
  ];
}

function asDateInput(value: string): string {
  if (/^\d{4}-\d{2}-\d{2}/.test(value)) return value.slice(0, 10);
  const md = /^(\d{2})-(\d{2})$/.exec(value);
  if (!md) return "";
  return `${new Date().getFullYear()}-${md[1]}-${md[2]}`;
}

function clockFrom(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function patternLabel(item: ScheduledItem, t: (english: string) => string): string {
  const days = (item.weekdays ?? [])
    .map((day) => {
      const label = WEEKDAYS.find((item) => item.id === day)?.label;
      return label ? t(label) : "";
    })
    .filter(Boolean)
    .join(" ");
  const every = Math.max(1, item.interval || 1);
  let base = "";
  if (item.recurrence === "daily") base = t("Daily");
  else if (item.recurrence === "weekly") base = days ? `${t("Weekly")} ${days}` : t("Weekly");
  else if (item.recurrence === "monthly") base = t("Monthly");
  else if (item.recurrence === "yearly") base = item.date ? `${t("Yearly")} ${item.date}` : t("Yearly");
  else if (item.recurrence === "once" && item.date) base = `${t("Once")} ${item.date}`;
  else if (item.recurrence === "every_n_days") base = fill("Every {n} days", { n: every });
  else if (item.recurrence === "every_n_weeks") base = fill("Every {n} weeks", { n: every });
  else if (item.recurrence === "every_n_months") base = fill("Every {n} months", { n: every });
  else if (item.recurrence === "every_n_years") base = fill("Every {n} years", { n: every });
  if (item.endCount) base = [base, fill("Ends after {n}", { n: item.endCount })].filter(Boolean).join(" · ");
  return base;
}

function waitingLine(grouped: ReturnType<typeof groupTimeItems>, t: (english: string) => string): string {
  const parts: string[] = [];
  if (grouped.overdue) parts.push(fill("{n} overdue", { n: grouped.overdue }));
  if (grouped.today) parts.push(fill("{n} today", { n: grouped.today }));
  if (!grouped.overdue && !grouped.today && grouped.later) parts.push(fill("{n} later", { n: grouped.later }));
  return parts.length ? parts.join(" · ") : t("None waiting");
}
