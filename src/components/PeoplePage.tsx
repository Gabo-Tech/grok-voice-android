import { useEffect, useMemo, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Trash2 } from "lucide-react";
import { EmptyState, Notice, PageChrome, RowButton } from "@/components/PageChrome";
import { hasTag, popTags, pushStep, requestBack } from "@/lib/history";
import { ShareButton } from "@/components/ShareButton";
import { Field } from "@/components/Field";
import { Sheet } from "@/components/Sheet";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  api,
  errorText,
  type Person,
  type PersonDraft,
  type PersonReminder,
  type PhoneContact,
  type PreferredChannel,
} from "@/lib/api";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

const CHANNELS: { id: PreferredChannel; label: string }[] = [
  { id: "", label: "None" },
  { id: "call", label: "Call" },
  { id: "sms", label: "Text" },
  { id: "whatsapp", label: "WhatsApp" },
  { id: "signal", label: "Signal" },
  { id: "email", label: "Email" },
];

type Draft = {
  id?: string;
  androidContactId: string | null;
  displayName: string;
  nickname: string;
  aliases: string;
  preferredChannel: PreferredChannel;
  phone: string;
  email: string;
  whatsapp: string;
  signal: string;
  favorite: boolean;
  note: string;
  reminders: PersonReminder[];
  reachOutDays: number;
  lastContactedMs: number | null;
};

const LABELS = [
  { id: "birthday", label: "Birthday" },
  { id: "anniversary", label: "Anniversary" },
  { id: "custom", label: "Custom" },
] as const;

function blankReminder(): PersonReminder {
  return { id: "", label: "birthday", title: "", date: "", recurrence: "yearly", leadDays: 0, time: "09:00", enabled: true };
}

function toDateInput(date: string): string {
  const trimmed = date.trim();
  const iso = /^(\d{4})-(\d{2})-(\d{2})$/.exec(trimmed);
  if (iso) return trimmed;
  const md = /^(\d{1,2})-(\d{1,2})$/.exec(trimmed);
  if (!md) return "";
  const month = md[1].padStart(2, "0");
  const day = md[2].padStart(2, "0");
  const year = new Date().getFullYear();
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const candidate = new Date(year, Number(month) - 1, Number(day));
  const shown = candidate < today ? year + 1 : year;
  return `${shown}-${month}-${day}`;
}

function shortDate(date: string): string {
  const iso = toDateInput(date);
  if (!iso) return date;
  const [year, month, day] = iso.split("-").map(Number);
  const showYear = /^\d{4}-/.test(date.trim());
  return new Date(year, month - 1, day).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: showYear ? "numeric" : undefined,
  });
}

function dateTitle(reminder: PersonReminder, t: (text: string) => string): string {
  if (reminder.label === "custom") return reminder.title.trim() || t("Custom");
  if (reminder.label === "anniversary") return t("Anniversary");
  if (reminder.label === "birthday") return t("Birthday");
  return reminder.title.trim() || t("Custom");
}

function payloadFrom(draft: Draft): PersonDraft {
  return {
    id: draft.id,
    androidContactId: draft.androidContactId,
    displayName: draft.displayName.trim(),
    nickname: draft.nickname.trim(),
    aliases: draft.aliases
      .split(/[,\n]/)
      .map((part) => part.trim())
      .filter(Boolean),
    preferredChannel: draft.preferredChannel,
    handles: {
      phone: draft.phone.trim(),
      email: draft.email.trim(),
      whatsapp: draft.whatsapp.trim(),
      signal: draft.signal.trim(),
    },
    favorite: draft.favorite,
    note: draft.note.trim(),
    reminders: draft.reminders.filter((reminder) => reminder.date.trim()),
    reachOutDays: draft.reachOutDays,
    lastContactedMs: draft.lastContactedMs,
  };
}

function emptyDraft(): Draft {
  return {
    androidContactId: null,
    displayName: "",
    nickname: "",
    aliases: "",
    preferredChannel: "",
    phone: "",
    email: "",
    whatsapp: "",
    signal: "",
    favorite: false,
    note: "",
    reminders: [],
    reachOutDays: 0,
    lastContactedMs: null,
  };
}

function draftFrom(person: Person): Draft {
  return {
    id: person.id,
    androidContactId: person.androidContactId,
    displayName: person.displayName,
    nickname: person.nickname,
    aliases: person.aliases.join(", "),
    preferredChannel: person.preferredChannel,
    phone: person.handles.phone,
    email: person.handles.email,
    whatsapp: person.handles.whatsapp,
    signal: person.handles.signal,
    favorite: person.favorite,
    note: person.note,
    reminders: person.reminders ?? [],
    reachOutDays: person.reachOutDays ?? 0,
    lastContactedMs: person.lastContactedMs ?? null,
  };
}

function byName(a: Person, b: Person) {
  return a.displayName.localeCompare(b.displayName, undefined, { sensitivity: "base" });
}

function channelLabel(channel: string) {
  return CHANNELS.find((item) => item.id === channel)?.label ?? "";
}

export function PeoplePage({ active = true }: { onBack?: () => void; active?: boolean }) {
  const t = useT();
  const [people, setPeople] = useState<Person[]>([]);
  const [query, setQuery] = useState("");
  const [screen, setScreen] = useState<"list" | "edit" | "pick" | "date">("list");
  const screenRef = useRef(screen);
  screenRef.current = screen;
  const goScreen = (next: typeof screen) => {
    const prev = screenRef.current;
    if (prev === next) return;
    pushStep(prev === "list" ? "Back to People" : "Back", () => {
      screenRef.current = prev;
      setScreen(prev);
    }, "people");
    screenRef.current = next;
    setScreen(next);
  };
  const backPeople = () => {
    if (!requestBack()) {
      screenRef.current = "list";
      setScreen("list");
    }
  };
  const closePeople = () => {
    if (hasTag("people")) {
      popTags(["people"]);
      return;
    }
    screenRef.current = "list";
    setScreen("list");
  };
  const [draft, setDraft] = useState<Draft>(emptyDraft());
  const [dateDraft, setDateDraft] = useState<PersonReminder | null>(null);
  const [dateIndex, setDateIndex] = useState<number | null>(null);
  const [linked, setLinked] = useState<PhoneContact | null>(null);
  const [adding, setAdding] = useState(false);
  const [pickFor, setPickFor] = useState<"create" | "link">("create");
  const [phoneQuery, setPhoneQuery] = useState("");
  const [phoneRows, setPhoneRows] = useState<PhoneContact[]>([]);
  const [contactsAllowed, setContactsAllowed] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = () => {
    if (!isTauri()) return;
    void api
      .listPeople()
      .then((rows) => {
        setPeople(rows);
        setError(null);
      })
      .catch((e) => setError(errorText(e)));
  };

  useEffect(() => {
    reload();
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void listen("voice://people", () => reload()).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (screen !== "pick" || contactsAllowed !== true) return;
    const timer = window.setTimeout(() => {
      void api
        .listPhoneContacts(phoneQuery)
        .then((rows) => {
          setPhoneRows(rows);
          setError(null);
        })
        .catch((e) => {
          const text = errorText(e);
          if (/not granted|permission/i.test(text)) setContactsAllowed(false);
          else setError(text);
        });
    }, 250);
    return () => window.clearTimeout(timer);
  }, [screen, phoneQuery, contactsAllowed]);

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const rows = people.filter((person) => {
      if (!needle) return true;
      const hay = [
        person.displayName,
        person.nickname,
        person.note,
        person.preferredChannel,
        person.handles.phone,
        person.handles.email,
        ...person.aliases,
      ]
        .join(" ")
        .toLowerCase();
      return hay.includes(needle);
    });
    return {
      favorites: rows.filter((person) => person.favorite).sort(byName),
      rest: rows.filter((person) => !person.favorite).sort(byName),
    };
  }, [people, query]);

  const openCreate = () => {
    setDraft(emptyDraft());
    setLinked(null);
    setError(null);
    setAdding(true);
  };

  const openEdit = (person: Person) => {
    setDraft(draftFrom(person));
    setLinked(null);
    setError(null);
    goScreen("edit");
    if (person.androidContactId) {
      void api
        .phoneContact(person.androidContactId)
        .then(setLinked)
        .catch(() => setLinked(null));
    }
  };

  const askContacts = async (query: string) => {
    setBusy(true);
    setError(null);
    try {
      const status = await api.requestContacts();
      if (status.contacts === "denied") {
        setContactsAllowed(false);
        setError(null);
        return;
      }
      const rows = await api.listPhoneContacts(query);
      setContactsAllowed(true);
      setPhoneRows(rows);
      setError(null);
    } catch (e) {
      const text = errorText(e);
      if (/not granted|permission/i.test(text)) {
        setContactsAllowed(false);
        setError(null);
      } else {
        setContactsAllowed(true);
        setPhoneRows([]);
        setError(text);
      }
    } finally {
      setBusy(false);
    }
  };

  const openPicker = (purpose: "create" | "link") => {
    setPickFor(purpose);
    setPhoneQuery("");
    setPhoneRows([]);
    setAdding(false);
    setContactsAllowed(null);
    goScreen("pick");
    void askContacts("");
  };

  const chooseContact = (contact: PhoneContact) => {
    setLinked(contact);
    setDraft((current) => {
      const base = pickFor === "create" ? emptyDraft() : current;
      return {
        ...base,
        androidContactId: contact.id,
        displayName: base.displayName.trim() || contact.name,
      };
    });
    goScreen("edit");
  };

  const openAddDate = () => {
    setDateDraft(blankReminder());
    setDateIndex(null);
    setError(null);
    goScreen("date");
  };

  const openEditDate = (index: number) => {
    setDateDraft({ ...draft.reminders[index] });
    setDateIndex(index);
    setError(null);
    goScreen("date");
  };

  const persistReminders = async (reminders: PersonReminder[]) => {
    const next = { ...draft, reminders };
    setDraft(next);
    if (!next.id || !next.displayName.trim()) {
      backPeople();
      setError(null);
      return;
    }
    setBusy(true);
    try {
      const saved = await api.savePerson(payloadFrom(next));
      setDraft(draftFrom(saved));
      reload();
      backPeople();
      setError(null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const saveDate = async () => {
    if (!dateDraft) return;
    if (!dateDraft.date.trim()) {
      setError("Each date needs a day.");
      return;
    }
    if (dateDraft.label === "custom" && !dateDraft.title.trim()) {
      setError("A custom date needs a name.");
      return;
    }
    const reminders = draft.reminders.slice();
    if (dateIndex == null) reminders.push(dateDraft);
    else reminders[dateIndex] = dateDraft;
    await persistReminders(reminders);
  };

  const removeDate = async () => {
    if (dateIndex == null) {
      backPeople();
      return;
    }
    await persistReminders(draft.reminders.filter((_, index) => index !== dateIndex));
  };

  const save = async () => {
    const displayName = draft.displayName.trim();
    if (!displayName) {
      setError("A person needs a name.");
      return;
    }
    if (draft.reminders.some((reminder) => reminder.label === "custom" && !reminder.title.trim())) {
      setError("A custom date needs a name.");
      return;
    }
    if (draft.reminders.some((reminder) => !reminder.date.trim())) {
      setError("Each date needs a day.");
      return;
    }
    const payload = payloadFrom({ ...draft, displayName });
    setBusy(true);
    try {
      await api.savePerson(payload);
      reload();
      closePeople();
      setError(null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!draft.id) return;
    setBusy(true);
    try {
      setPeople(await api.deletePerson(draft.id));
      closePeople();
      setError(null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (screen === "pick") {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <PageChrome title={t("Phone contacts")} subtitle={t("Pick someone. Their number stays in Contacts.")} active={false}>
          <div className="mb-6">
            <Field label={t("Search")}>
              <Input
                value={phoneQuery}
                onChange={(e) => setPhoneQuery(e.target.value)}
                placeholder={t("Name in Contacts")}
                aria-label={t("Search phone contacts")}
              />
            </Field>
          </div>
          {error && <Notice title={t(error)} />}
          {contactsAllowed === false ? (
            <div className="px-1">
              <EmptyState title={t("Contacts access is off")} body={t("Allow it to pick a phone contact, or go back and add someone without one.")} />
              <Button disabled={busy} onClick={() => void askContacts(phoneQuery)}>
                {t("Allow contacts")}
              </Button>
            </div>
          ) : phoneRows.length === 0 ? (
            <EmptyState
              title={contactsAllowed === null || busy ? t("Looking up contacts") : phoneQuery ? t("No matches") : t("No phone contacts")}
              body={
                contactsAllowed === null || busy
                  ? t("This stays on the phone.")
                  : t("Try another name, or add someone without a phone contact.")
              }
            />
          ) : (
            <div className="divide-y divide-border border-y border-border">
              {phoneRows.map((contact) => (
                <RowButton key={contact.id} onClick={() => chooseContact(contact)}>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-base leading-6 text-foreground">{contact.name || t("Unnamed")}</p>
                    <p className="mt-1 truncate text-sm leading-5 text-muted-foreground">
                      {[contact.phone, contact.email].filter(Boolean).join(" · ") || t("No phone or email")}
                    </p>
                  </div>
                </RowButton>
              ))}
            </div>
          )}
        </PageChrome>
      </div>
    );
  }

  if (screen === "date" && dateDraft) {
    return (
      <DateEditor
        reminder={dateDraft}
        busy={busy}
        error={error}
        isNew={dateIndex == null}
        onChange={setDateDraft}
        onSave={() => void saveDate()}
        onRemove={() => void removeDate()}
        onBack={() => {
          setError(null);
          backPeople();
        }}
      />
    );
  }

  if (screen === "edit") {
    const linkedLine = [linked?.phone, linked?.email].filter(Boolean).join(" · ");
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="min-h-0 flex-1 overflow-y-auto pb-3">
          <h2 className="px-1 text-2xl font-medium tracking-tight">{draft.id ? draft.displayName || t("Person") : t("New person")}</h2>
          <p className="mt-1 px-1 text-xs leading-5 text-muted-foreground">
            {t("This record stays on the phone. It does not change the Contacts app.")}
          </p>
          <div className="mt-6 space-y-4">
            <h3 className="text-base text-foreground">{t("Who")}</h3>
            <Field label={t("Name")} error={screen === "edit" && error ? t(error) : null}>
              <Input value={draft.displayName} onChange={(e) => setDraft({ ...draft, displayName: e.target.value })} placeholder={t("Ana Lopez")} />
            </Field>
            <DateList
              reminders={draft.reminders}
              onAdd={openAddDate}
              onOpen={openEditDate}
              onImport={
                draft.androidContactId && draft.id
                  ? () => {
                      void api
                        .importContactDates(draft.id || "")
                        .then((person) => {
                          setDraft(draftFrom(person));
                          reload();
                        })
                        .catch((e) => setError(errorText(e)));
                    }
                  : undefined
              }
            />
            <Field label={t("Nickname")}>
              <Input value={draft.nickname} onChange={(e) => setDraft({ ...draft, nickname: e.target.value })} placeholder={t("Mom")} />
            </Field>
            <Field label={t("Aliases")} hint={t("Comma separated. Voice matches these too.")}>
              <Input value={draft.aliases} onChange={(e) => setDraft({ ...draft, aliases: e.target.value })} placeholder={t("mummy, ma")} />
            </Field>
            <h3 className="pt-2 text-base text-foreground">{t("How you reach them")}</h3>
            <div>
              <p className="mb-2 text-sm text-muted-foreground">{t("Preferred channel")}</p>
              <div className="grid grid-cols-3 gap-2">
                {CHANNELS.map((item) => (
                  <button
                    key={item.id || "none"}
                    type="button"
                    onClick={() => setDraft({ ...draft, preferredChannel: item.id })}
                    className={cn(
                      "min-h-12 h-auto border px-1 py-2 text-center text-sm leading-4 whitespace-normal",
                      draft.preferredChannel === item.id ? "border-foreground text-foreground" : "border-border text-muted-foreground",
                    )}
                  >
                    {t(item.label)}
                  </button>
                ))}
              </div>
            </div>
            <Field label={t("Phone override")} hint={t("Leave blank to use the phone contact.")}>
              <Input value={draft.phone} onChange={(e) => setDraft({ ...draft, phone: e.target.value })} placeholder="+1 555 0100" />
            </Field>
            <Field label={t("Email override")}>
              <Input value={draft.email} onChange={(e) => setDraft({ ...draft, email: e.target.value })} placeholder="ana@example.com" />
            </Field>
            <Field label={t("WhatsApp number")}>
              <Input value={draft.whatsapp} onChange={(e) => setDraft({ ...draft, whatsapp: e.target.value })} placeholder={t("Uses the phone number if empty")} />
            </Field>
            <Field label={t("Signal number")}>
              <Input value={draft.signal} onChange={(e) => setDraft({ ...draft, signal: e.target.value })} placeholder={t("Uses the phone number if empty")} />
            </Field>
            <button
              type="button"
              onClick={() => setDraft({ ...draft, favorite: !draft.favorite })}
              className={cn(
                "flex h-12 w-full items-center justify-between border px-3 text-left text-sm",
                draft.favorite ? "border-foreground text-foreground" : "border-border text-muted-foreground",
              )}
            >
              <span>{t("Favorite")}</span>
              <span>{draft.favorite ? t("Pinned") : t("Off")}</span>
            </button>
            <Field label={t("Reach out every")}>
              <Input
                inputMode="numeric"
                value={draft.reachOutDays ? String(draft.reachOutDays) : ""}
                placeholder={t("Off")}
                onChange={(e) => setDraft({ ...draft, reachOutDays: Math.max(0, Math.min(365, Number(e.target.value) || 0)) })}
              />
            </Field>
            <p className="text-sm leading-5 text-muted-foreground">
              {draft.lastContactedMs
                ? `${t("Last call or text")}: ${new Date(draft.lastContactedMs).toLocaleString()}`
                : t("Last call or text")}
            </p>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="outline"
                disabled={!draft.id}
                onClick={() => {
                  if (!draft.id) return;
                  void api
                    .requestHistoryAccess()
                    .then(() => api.refreshLastContact(draft.id || ""))
                    .then((person) => setDraft(draftFrom(person)))
                    .catch((e) => setError(errorText(e)));
                }}
              >
                {t("Use call and text history")}
              </Button>
            </div>
            <h3 className="pt-2 text-base text-foreground">{t("Note")}</h3>
            <Field label={t("Note")}>
              <Textarea value={draft.note} onChange={(e) => setDraft({ ...draft, note: e.target.value })} placeholder={t("Call after 5")} className="min-h-28" />
            </Field>
            <div className="border border-border px-3 py-3">
              <p className="text-sm text-muted-foreground">{t("Phone contact")}</p>
              {draft.androidContactId ? (
                <>
                  <p className="mt-2 text-sm text-foreground">{linked?.name || t("Linked")}</p>
                  {linkedLine && <p className="mt-1 text-sm text-muted-foreground">{linkedLine}</p>}
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button
                      variant="outline"
                      onClick={() => void api.openPhoneContact(draft.androidContactId || "").catch((e) => setError(errorText(e)))}
                    >
                      {t("Open in Contacts")}
                    </Button>
                    <Button
                      variant="ghost"
                      onClick={() => {
                        setDraft({ ...draft, androidContactId: null });
                        setLinked(null);
                      }}
                    >
                      {t("Unlink")}
                    </Button>
                  </div>
                </>
              ) : (
                <>
                  <p className="mt-2 text-sm text-muted-foreground">{t("No phone contact. This person lives only in Grok Voice.")}</p>
                  <Button className="mt-3" variant="outline" onClick={() => openPicker("link")}>
                    {t("Link a phone contact")}
                  </Button>
                </>
              )}
            </div>
          </div>
        </div>
        <div className="thumb-actions flex shrink-0 flex-wrap items-center gap-2 border-t border-border bg-background py-3">
          <ShareButton
            title={draft.displayName}
            text={[draft.nickname, draft.preferredChannel ? t(channelLabel(draft.preferredChannel)) : "", draft.phone, draft.email, draft.note].filter(Boolean).join("\n")}
          />
          {draft.id && (
            <Button variant="ghost" disabled={busy} onClick={() => void remove()}>
              <Trash2 /> {t("Delete")}
            </Button>
          )}
          <div className="flex-1" />
          <Button variant="ghost" onClick={backPeople}>
            {t("Cancel")}
          </Button>
          <Button disabled={busy || !draft.displayName.trim()} onClick={() => void save()}>
            {t("Save")}
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageChrome title={t("People")} subtitle={`${people.length} ${t("saved on this phone")}`} onAdd={openCreate} addLabel={t("Add person")} active={active && screen === "list"}>
        <p className="mb-6 px-1 text-sm leading-6 text-muted-foreground">
          {t("Nicknames and how you reach someone. Phone numbers and email still come from Contacts.")}
        </p>
        {(people.length > 0 || query) && (
        <div className="mb-6">
          <Field label={t("Search")}>
            <div className="relative">
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("Name, nickname, or alias")}
                aria-label={t("Search people")}
                className="pr-16"
              />
              {query && (
                <button
                  type="button"
                  className="absolute top-0 right-0 h-12 px-3 text-sm text-foreground"
                  onClick={() => setQuery("")}
                >
                  {t("Clear")}
                </button>
              )}
            </div>
          </Field>
        </div>
        )}
        {error && <Notice title={t(error)} />}
        {visible.favorites.length === 0 && visible.rest.length === 0 ? (
          <EmptyState
            title={query ? t("No matches") : t("No people yet")}
            body={query ? t("Try another name.") : t("Tap + to pick a phone contact, or add someone with only a nickname.")}
            actionLabel={query ? t("Clear search") : t("Add person")}
            onAction={query ? () => setQuery("") : openCreate}
          />
        ) : (
          <div className="space-y-6">
            {visible.favorites.length > 0 && <PersonGroup title={t("Favorites")} people={visible.favorites} onOpen={openEdit} />}
            {visible.rest.length > 0 && <PersonGroup title={t("A–Z")} people={visible.rest} onOpen={openEdit} />}
          </div>
        )}
      </PageChrome>
      <Sheet
        open={adding}
        title={t("Add person")}
        onClose={() => setAdding(false)}
        footer={
          <Button variant="ghost" onClick={() => setAdding(false)}>
            {t("Cancel")}
          </Button>
        }
      >
        <p className="mb-4 text-sm leading-6 text-muted-foreground">
          {t("Pick someone from the phone, or keep them only in Grok Voice. Deleting here does not delete the contact.")}
        </p>
        <div className="divide-y divide-border border-y border-border">
          <RowButton
            onClick={() => {
              setAdding(false);
              openPicker("create");
            }}
          >
            <div>
              <p className="text-base text-foreground">{t("From phone contacts")}</p>
              <p className="mt-1 text-sm text-muted-foreground">{t("Asks for contacts access if it is off.")}</p>
            </div>
          </RowButton>
          <RowButton
            onClick={() => {
              setDraft(emptyDraft());
              setLinked(null);
              setAdding(false);
              goScreen("edit");
            }}
          >
            <div>
              <p className="text-base text-foreground">{t("Without a phone contact")}</p>
              <p className="mt-1 text-sm text-muted-foreground">{t("A nickname and a number you type here.")}</p>
            </div>
          </RowButton>
        </div>
      </Sheet>
    </div>
  );
}

function PersonGroup({ title, people, onOpen }: { title: string; people: Person[]; onOpen: (person: Person) => void }) {
  const t = useT();
  return (
    <section>
      <p className="mb-2 px-1 text-sm text-muted-foreground">{title}</p>
      <div className="divide-y divide-border border-y border-border">
        {people.map((person) => {
          const aka = person.nickname || person.aliases[0] || "";
          const channel = person.preferredChannel ? t(channelLabel(person.preferredChannel)) : "";
          const dated = (person.reminders ?? [])
            .filter((reminder) => reminder.enabled)
            .slice(0, 2)
            .map((reminder) => `${dateTitle(reminder, t)} ${shortDate(reminder.date)}`.trim());
          const detail = [aka, channel, ...dated].filter(Boolean).join(" · ");
          return (
            <RowButton key={person.id} onClick={() => onOpen(person)} className="items-center">
              <span className="grid size-12 shrink-0 place-items-center border border-border text-base text-foreground" aria-hidden>
                {(person.displayName.trim()[0] || "?").toUpperCase()}
              </span>
              <div className="min-w-0 flex-1">
                <p className="truncate text-base leading-6 text-foreground">{person.displayName}</p>
                <p className="mt-1 truncate text-sm leading-5 text-muted-foreground">
                  {detail || (person.androidContactId ? t("Phone contact") : t("App only"))}
                </p>
              </div>
            </RowButton>
          );
        })}
      </div>
    </section>
  );
}

function DateList({
  reminders,
  onAdd,
  onOpen,
  onImport,
}: {
  reminders: PersonReminder[];
  onAdd: () => void;
  onOpen: (index: number) => void;
  onImport?: () => void;
}) {
  const t = useT();
  return (
    <section className="border border-border px-3 py-3">
      <h3 className="text-base text-foreground">{t("Dates")}</h3>
      <p className="mt-1 text-sm leading-5 text-muted-foreground">{t("Birthday, anniversary, or a date you name.")}</p>
      {reminders.length === 0 ? (
        <p className="mt-3 text-sm leading-5 text-muted-foreground">{t("No dates yet")}</p>
      ) : (
        <div className="mt-3 divide-y divide-border border-y border-border">
          {reminders.map((reminder, index) => (
            <button
              key={`${reminder.id || "new"}-${index}`}
              type="button"
              onClick={() => onOpen(index)}
              className="flex min-h-14 w-full items-center justify-between gap-3 py-3 text-left"
            >
              <span className="min-w-0">
                <span className="block truncate text-base text-foreground">{dateTitle(reminder, t)}</span>
                <span className="mt-1 block text-sm text-muted-foreground">
                  {shortDate(reminder.date) || t("Date")}
                  {" · "}
                  {reminder.recurrence === "once" ? t("Just once") : t("Every year")}
                  {reminder.enabled ? "" : ` · ${t("Off")}`}
                </span>
              </span>
              <span className="shrink-0 text-sm text-foreground" aria-hidden>
                {t("Edit date")}
              </span>
            </button>
          ))}
        </div>
      )}
      <button type="button" onClick={onAdd} className="mt-3 h-14 w-full border border-foreground text-base text-foreground">
        {t("Add a date")}
      </button>
      {onImport && (
        <button type="button" onClick={onImport} className="mt-2 h-12 w-full border border-border text-sm text-foreground">
          {t("Import dates")}
        </button>
      )}
    </section>
  );
}

function DateEditor({
  reminder,
  busy,
  error,
  isNew,
  onChange,
  onSave,
  onRemove,
  onBack,
}: {
  reminder: PersonReminder;
  busy: boolean;
  error: string | null;
  isNew: boolean;
  onChange: (reminder: PersonReminder) => void;
  onSave: () => void;
  onRemove: () => void;
  onBack: () => void;
}) {
  const t = useT();
  const lead = [
    { days: 0, label: t("Day of") },
    { days: 1, label: t("1 day before") },
    { days: 3, label: t("3 days before") },
    { days: 7, label: t("7 days before") },
  ];
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto pb-3">
        <h2 className="px-1 text-2xl font-medium tracking-tight">{isNew ? t("Add a date") : t("Edit date")}</h2>
        <p className="mt-1 px-1 text-sm leading-5 text-muted-foreground">
          {t("Pick a type, then the day. It reminds you on this phone and shows in Time.")}
        </p>
        <div className="mt-6 space-y-5">
          <div>
            <p className="mb-2 text-sm text-muted-foreground" id="date-type-label">
              {t("Dates")}
            </p>
            <div className="grid grid-cols-3 gap-2" role="group" aria-labelledby="date-type-label">
              {LABELS.map((item) => {
                const selected = reminder.label === item.id;
                return (
                  <button
                    key={item.id}
                    type="button"
                    aria-pressed={selected}
                    onClick={() => onChange({ ...reminder, label: item.id })}
                    className={cn(
                      "h-14 border text-sm",
                      selected ? "border-2 border-foreground font-medium text-foreground" : "border-border text-muted-foreground",
                    )}
                  >
                    {t(item.label)}
                  </button>
                );
              })}
            </div>
          </div>
          {reminder.label === "custom" && (
            <Field label={t("Name this date")} error={error && !reminder.title.trim() ? t(error) : null}>
              <Input
                value={reminder.title}
                aria-label={t("Name this date")}
                onChange={(e) => onChange({ ...reminder, title: e.target.value })}
                placeholder={t("Custom")}
              />
            </Field>
          )}
          {error && reminder.date.trim() && (reminder.label !== "custom" || reminder.title.trim()) && (
            <p role="alert" className="text-sm leading-5 text-foreground">
              {t(error)}
            </p>
          )}
          <Field label={t("Date")} error={error && !reminder.date.trim() ? t(error) : null}>
            <Input
              type="date"
              value={toDateInput(reminder.date)}
              aria-label={t("Date")}
              onChange={(e) => onChange({ ...reminder, date: e.target.value })}
            />
          </Field>
          <Field label={t("Clock")}>
            <Input
              type="time"
              value={reminder.time || "09:00"}
              aria-label={t("Clock")}
              onChange={(e) => onChange({ ...reminder, time: e.target.value })}
            />
          </Field>
          <div>
            <p className="mb-2 text-sm text-muted-foreground">{t("Repeats")}</p>
            <div className="grid grid-cols-2 gap-2">
              {(["yearly", "once"] as const).map((item) => {
                const selected = reminder.recurrence === item;
                return (
                  <button
                    key={item}
                    type="button"
                    aria-pressed={selected}
                    onClick={() => onChange({ ...reminder, recurrence: item })}
                    className={cn(
                      "h-14 border text-sm",
                      selected ? "border-2 border-foreground font-medium text-foreground" : "border-border text-muted-foreground",
                    )}
                  >
                    {item === "yearly" ? t("Every year") : t("Just once")}
                  </button>
                );
              })}
            </div>
          </div>
          <div>
            <p className="mb-2 text-sm text-muted-foreground">{t("Notify")}</p>
            <div className="grid grid-cols-2 gap-2">
              {lead.map((item) => {
                const selected = reminder.leadDays === item.days;
                return (
                  <button
                    key={item.days}
                    type="button"
                    aria-pressed={selected}
                    onClick={() => onChange({ ...reminder, leadDays: item.days })}
                    className={cn(
                      "h-14 border px-2 text-sm",
                      selected ? "border-2 border-foreground font-medium text-foreground" : "border-border text-muted-foreground",
                    )}
                  >
                    {item.label}
                  </button>
                );
              })}
            </div>
          </div>
          <button
            type="button"
            aria-pressed={reminder.enabled}
            onClick={() => onChange({ ...reminder, enabled: !reminder.enabled })}
            className={cn(
              "flex h-14 w-full items-center justify-between border px-3 text-left text-base",
              reminder.enabled ? "border-foreground text-foreground" : "border-border text-muted-foreground",
            )}
          >
            <span>{t("Notify")}</span>
            <span>{reminder.enabled ? t("On") : t("Off")}</span>
          </button>
        </div>
      </div>
      <div className="thumb-actions flex shrink-0 flex-wrap items-center gap-2 border-t border-border bg-background py-3">
        {!isNew && (
          <Button variant="ghost" disabled={busy} onClick={onRemove}>
            {t("Remove date")}
          </Button>
        )}
        <div className="flex-1" />
        <Button variant="ghost" onClick={onBack}>
          {t("Cancel")}
        </Button>
        <Button disabled={busy} onClick={onSave}>
          {t("Save date")}
        </Button>
      </div>
    </div>
  );
}

