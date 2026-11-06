import { useEffect, useMemo, useState } from "react";
import { Trash2 } from "lucide-react";
import { BackButton, EmptyState, Notice, PageChrome, RowButton } from "@/components/PageChrome";
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

export function PeoplePage({ onBack }: { onBack: () => void }) {
  const t = useT();
  const [people, setPeople] = useState<Person[]>([]);
  const [query, setQuery] = useState("");
  const [screen, setScreen] = useState<"list" | "edit" | "pick">("list");
  const [draft, setDraft] = useState<Draft>(emptyDraft());
  const [linked, setLinked] = useState<PhoneContact | null>(null);
  const [adding, setAdding] = useState(false);
  const [pickFor, setPickFor] = useState<"create" | "link">("create");
  const [phoneQuery, setPhoneQuery] = useState("");
  const [phoneRows, setPhoneRows] = useState<PhoneContact[]>([]);
  const [contactsAllowed, setContactsAllowed] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = () => {
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
    setScreen("edit");
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
    setScreen("pick");
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
    setScreen("edit");
  };

  const save = async () => {
    const displayName = draft.displayName.trim();
    if (!displayName) {
      setError("A person needs a name.");
      return;
    }
    if (draft.reminders.some((reminder) => !reminder.date.trim())) {
      setError("Each date needs a month and day, like 03-03.");
      return;
    }
    const payload: PersonDraft = {
      id: draft.id,
      androidContactId: draft.androidContactId,
      displayName,
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
    setBusy(true);
    try {
      await api.savePerson(payload);
      reload();
      setScreen("list");
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
      setScreen("list");
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
        <BackButton
          label={draft.displayName || draft.id ? "Back to person" : "Back to People"}
          onClick={() => {
            setError(null);
            setScreen(draft.displayName || draft.id ? "edit" : "list");
          }}
        />
        <PageChrome title="Phone contacts" subtitle="Pick someone. Their number stays in Contacts.">
          <div className="mb-6">
            <Field label="Search">
              <Input
                value={phoneQuery}
                onChange={(e) => setPhoneQuery(e.target.value)}
                placeholder="Name in Contacts"
                aria-label="Search phone contacts"
              />
            </Field>
          </div>
          {error && <Notice title={error} />}
          {contactsAllowed === false ? (
            <div className="px-1">
              <EmptyState title="Contacts access is off" body="Allow it to pick a phone contact, or go back and add someone without one." />
              <Button disabled={busy} onClick={() => void askContacts(phoneQuery)}>
                Allow contacts
              </Button>
            </div>
          ) : phoneRows.length === 0 ? (
            <EmptyState
              title={contactsAllowed === null || busy ? "Looking up contacts" : phoneQuery ? "No matches" : "No phone contacts"}
              body={
                contactsAllowed === null || busy
                  ? "This stays on the phone."
                  : "Try another name, or add someone without a phone contact."
              }
            />
          ) : (
            <div className="divide-y divide-border border-y border-border">
              {phoneRows.map((contact) => (
                <RowButton key={contact.id} onClick={() => chooseContact(contact)}>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-base leading-6 text-foreground">{contact.name || "Unnamed"}</p>
                    <p className="mt-1 truncate text-sm leading-5 text-muted-foreground">
                      {[contact.phone, contact.email].filter(Boolean).join(" · ") || "No phone or email"}
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

  if (screen === "edit") {
    const linkedLine = [linked?.phone, linked?.email].filter(Boolean).join(" · ");
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <BackButton label="Back to People" onClick={() => setScreen("list")} />
        <div className="min-h-0 flex-1 overflow-y-auto pb-3">
          <h2 className="px-1 text-2xl font-medium tracking-tight">{draft.id ? draft.displayName || "Person" : "New person"}</h2>
          <p className="mt-1 px-1 text-xs leading-5 text-muted-foreground">
            This record stays on the phone. It does not change the Contacts app.
          </p>
          <div className="mt-6 space-y-4">
            <h3 className="text-base text-foreground">Who</h3>
            <Field label="Name" error={error}>
              <Input value={draft.displayName} onChange={(e) => setDraft({ ...draft, displayName: e.target.value })} placeholder="Ana Lopez" />
            </Field>
            <Field label="Nickname">
              <Input value={draft.nickname} onChange={(e) => setDraft({ ...draft, nickname: e.target.value })} placeholder="Mom" />
            </Field>
            <Field label="Aliases" hint="Comma separated. Voice matches these too.">
              <Input value={draft.aliases} onChange={(e) => setDraft({ ...draft, aliases: e.target.value })} placeholder="mummy, ma" />
            </Field>
            <h3 className="pt-2 text-base text-foreground">{t("How you reach them")}</h3>
            <div>
              <p className="mb-2 text-sm text-muted-foreground">Preferred channel</p>
              <div className="grid grid-cols-3 gap-2">
                {CHANNELS.map((item) => (
                  <button
                    key={item.id || "none"}
                    type="button"
                    onClick={() => setDraft({ ...draft, preferredChannel: item.id })}
                    className={cn(
                      "h-12 border px-2 text-sm",
                      draft.preferredChannel === item.id ? "border-foreground text-foreground" : "border-border text-muted-foreground",
                    )}
                  >
                    {item.label}
                  </button>
                ))}
              </div>
            </div>
            <Field label="Phone override" hint="Leave blank to use the phone contact.">
              <Input value={draft.phone} onChange={(e) => setDraft({ ...draft, phone: e.target.value })} placeholder="+1 555 0100" />
            </Field>
            <Field label="Email override">
              <Input value={draft.email} onChange={(e) => setDraft({ ...draft, email: e.target.value })} placeholder="ana@example.com" />
            </Field>
            <Field label="WhatsApp number">
              <Input value={draft.whatsapp} onChange={(e) => setDraft({ ...draft, whatsapp: e.target.value })} placeholder="Uses the phone number if empty" />
            </Field>
            <Field label="Signal number">
              <Input value={draft.signal} onChange={(e) => setDraft({ ...draft, signal: e.target.value })} placeholder="Uses the phone number if empty" />
            </Field>
            <button
              type="button"
              onClick={() => setDraft({ ...draft, favorite: !draft.favorite })}
              className={cn(
                "flex h-12 w-full items-center justify-between border px-3 text-left text-sm",
                draft.favorite ? "border-foreground text-foreground" : "border-border text-muted-foreground",
              )}
            >
              <span>Favorite</span>
              <span>{draft.favorite ? "Pinned" : "Off"}</span>
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
              {draft.androidContactId && (
                <Button
                  type="button"
                  variant="outline"
                  disabled={!draft.id}
                  onClick={() => {
                    if (!draft.id) return;
                    void api
                      .importContactBirthday(draft.id)
                      .then((person) => setDraft(draftFrom(person)))
                      .catch((e) => setError(errorText(e)));
                  }}
                >
                  {t("Import birthday")}
                </Button>
              )}
            </div>
            <h3 className="pt-2 text-base text-foreground">{t("Note")}</h3>
            <Field label={t("Note")}>
              <Textarea value={draft.note} onChange={(e) => setDraft({ ...draft, note: e.target.value })} placeholder="Call after 5" className="min-h-28" />
            </Field>
            <div>
              <h3 className="text-base text-foreground">Dates</h3>
              <div className="mb-2 flex items-center justify-between gap-3">
                <p className="text-sm text-muted-foreground">Birthdays and anniversaries</p>
                <button
                  type="button"
                  className="h-12 border border-border px-3 text-sm"
                  onClick={() => setDraft({ ...draft, reminders: [...draft.reminders, blankReminder()] })}
                >
                  Add
                </button>
              </div>
              <p className="mb-3 text-xs leading-5 text-muted-foreground">
                Birthdays and anniversaries notify on this phone and show on Time. Nothing is opened.
              </p>
              <div className="space-y-3">
                {draft.reminders.map((reminder, index) => (
                  <ReminderEditor
                    key={`${reminder.id}-${index}`}
                    reminder={reminder}
                    onChange={(next) => {
                      const reminders = draft.reminders.slice();
                      reminders[index] = next;
                      setDraft({ ...draft, reminders });
                    }}
                    onRemove={() => setDraft({ ...draft, reminders: draft.reminders.filter((_, i) => i !== index) })}
                  />
                ))}
              </div>
            </div>
            <div className="border border-border px-3 py-3">
              <p className="text-sm text-muted-foreground">Phone contact</p>
              {draft.androidContactId ? (
                <>
                  <p className="mt-2 text-sm text-foreground">{linked?.name || "Linked"}</p>
                  {linkedLine && <p className="mt-1 text-sm text-muted-foreground">{linkedLine}</p>}
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button
                      variant="outline"
                      onClick={() => void api.openPhoneContact(draft.androidContactId || "").catch((e) => setError(errorText(e)))}
                    >
                      Open in Contacts
                    </Button>
                    <Button
                      variant="ghost"
                      onClick={() => {
                        setDraft({ ...draft, androidContactId: null });
                        setLinked(null);
                      }}
                    >
                      Unlink
                    </Button>
                  </div>
                </>
              ) : (
                <>
                  <p className="mt-2 text-sm text-muted-foreground">No phone contact. This person lives only in Grok Voice.</p>
                  <Button className="mt-3" variant="outline" onClick={() => openPicker("link")}>
                    Link a phone contact
                  </Button>
                </>
              )}
            </div>
          </div>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-2 border-t border-border py-3">
          <ShareButton
            title={draft.displayName}
            text={[draft.nickname, channelLabel(draft.preferredChannel), draft.phone, draft.email, draft.note].filter(Boolean).join("\n")}
          />
          {draft.id && (
            <Button variant="ghost" disabled={busy} onClick={() => void remove()}>
              <Trash2 /> Delete overlay
            </Button>
          )}
          <div className="flex-1" />
          <Button variant="ghost" onClick={() => setScreen("list")}>
            Cancel
          </Button>
          <Button disabled={busy || !draft.displayName.trim()} onClick={() => void save()}>
            Save
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <BackButton label={t("Back to Settings")} onClick={onBack} />
      <PageChrome title={t("People")} subtitle={`${people.length} ${t("saved on this phone")}`} onAdd={openCreate} addLabel={t("Add person")}>
        <p className="mb-6 px-1 text-sm leading-6 text-muted-foreground">
          Nicknames and how you reach someone. Phone numbers and email still come from Contacts.
        </p>
        {(people.length > 0 || query) && (
        <div className="mb-6">
          <Field label="Search">
            <div className="relative">
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Name, nickname, or alias"
                aria-label="Search people"
                className="pr-16"
              />
              {query && (
                <button
                  type="button"
                  className="absolute top-0 right-0 h-12 px-3 text-sm text-foreground"
                  onClick={() => setQuery("")}
                >
                  Clear
                </button>
              )}
            </div>
          </Field>
        </div>
        )}
        {error && <Notice title={error} />}
        {visible.favorites.length === 0 && visible.rest.length === 0 ? (
          <EmptyState
            title={query ? t("No matches") : t("No people yet")}
            body={query ? "Try another name." : "Tap + to pick a phone contact, or add someone with only a nickname."}
            actionLabel={query ? "Clear search" : "Add person"}
            onAction={query ? () => setQuery("") : openCreate}
          />
        ) : (
          <div className="space-y-6">
            {visible.favorites.length > 0 && <PersonGroup title={t("Favorites")} people={visible.favorites} onOpen={openEdit} />}
            {visible.rest.length > 0 && <PersonGroup title="A–Z" people={visible.rest} onOpen={openEdit} />}
          </div>
        )}
      </PageChrome>
      <Sheet
        open={adding}
        title="Add person"
        onClose={() => setAdding(false)}
        footer={
          <Button variant="ghost" onClick={() => setAdding(false)}>
            Cancel
          </Button>
        }
      >
        <p className="mb-4 text-sm leading-6 text-muted-foreground">
          Pick someone from the phone, or keep them only in Grok Voice. Deleting here does not delete the contact.
        </p>
        <div className="divide-y divide-border border-y border-border">
          <RowButton
            onClick={() => {
              setAdding(false);
              openPicker("create");
            }}
          >
            <div>
              <p className="text-base text-foreground">From phone contacts</p>
              <p className="mt-1 text-sm text-muted-foreground">Asks for contacts access if it is off.</p>
            </div>
          </RowButton>
          <RowButton
            onClick={() => {
              setDraft(emptyDraft());
              setLinked(null);
              setAdding(false);
              setScreen("edit");
            }}
          >
            <div>
              <p className="text-base text-foreground">Without a phone contact</p>
              <p className="mt-1 text-sm text-muted-foreground">A nickname and a number you type here.</p>
            </div>
          </RowButton>
        </div>
      </Sheet>
    </div>
  );
}

function PersonGroup({ title, people, onOpen }: { title: string; people: Person[]; onOpen: (person: Person) => void }) {
  return (
    <section>
      <p className="mb-2 px-1 text-sm text-muted-foreground">{title}</p>
      <div className="divide-y divide-border border-y border-border">
        {people.map((person) => {
          const aka = person.nickname || person.aliases[0] || "";
          const channel = channelLabel(person.preferredChannel);
          const dated = person.reminders?.find((reminder) => reminder.enabled)?.label ?? "";
          const detail = [aka, channel && channel !== "None" ? channel : "", dated].filter(Boolean).join(" · ");
          return (
            <RowButton key={person.id} onClick={() => onOpen(person)} className="items-center">
              <span className="grid size-12 shrink-0 place-items-center border border-border text-base text-foreground" aria-hidden>
                {(person.displayName.trim()[0] || "?").toUpperCase()}
              </span>
              <div className="min-w-0 flex-1">
                <p className="truncate text-base leading-6 text-foreground">{person.displayName}</p>
                <p className="mt-1 truncate text-sm leading-5 text-muted-foreground">
                  {detail || (person.androidContactId ? "Phone contact" : "App only")}
                </p>
              </div>
            </RowButton>
          );
        })}
      </div>
    </section>
  );
}

function ReminderEditor({
  reminder,
  onChange,
  onRemove,
}: {
  reminder: PersonReminder;
  onChange: (reminder: PersonReminder) => void;
  onRemove: () => void;
}) {
  return (
    <div className="space-y-3 border border-border p-3">
      <div className="grid grid-cols-3 gap-2">
        {LABELS.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => onChange({ ...reminder, label: item.id })}
            className={cn(
              "h-12 border text-sm",
              reminder.label === item.id ? "border-foreground text-foreground" : "border-border text-muted-foreground",
            )}
          >
            {item.label}
          </button>
        ))}
      </div>
      {reminder.label === "custom" && (
        <Input value={reminder.title} onChange={(e) => onChange({ ...reminder, title: e.target.value })} placeholder="Title" />
      )}
      <Input value={reminder.date} onChange={(e) => onChange({ ...reminder, date: e.target.value })} placeholder="03-03" aria-label="Month and day" />
      <div className="grid grid-cols-2 gap-2">
        {(["yearly", "once"] as const).map((item) => (
          <button
            key={item}
            type="button"
            onClick={() => onChange({ ...reminder, recurrence: item })}
            className={cn(
              "h-12 border text-sm",
              reminder.recurrence === item ? "border-foreground text-foreground" : "border-border text-muted-foreground",
            )}
          >
            {item}
          </button>
        ))}
      </div>
      <Input type="time" value={reminder.time || "09:00"} onChange={(e) => onChange({ ...reminder, time: e.target.value })} aria-label="Time" />
      <div className="flex gap-2">
        {[0, 1, 3, 7].map((days) => (
          <button
            key={days}
            type="button"
            onClick={() => onChange({ ...reminder, leadDays: days })}
            className={cn(
              "h-12 flex-1 border text-sm",
              reminder.leadDays === days ? "border-foreground text-foreground" : "border-border text-muted-foreground",
            )}
          >
            {days === 0 ? "Day of" : `${days}d early`}
          </button>
        ))}
      </div>
      <div className="flex items-center justify-between gap-3">
        <button
          type="button"
          className="min-h-12 px-2 text-sm text-foreground"
          onClick={() => onChange({ ...reminder, enabled: !reminder.enabled })}
        >
          {reminder.enabled ? "On" : "Off"}
        </button>
        <button type="button" className="min-h-12 px-2 text-sm" onClick={onRemove}>
          Remove
        </button>
      </div>
    </div>
  );
}

