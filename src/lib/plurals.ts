/** Count phrases for the restore summary. English, Spanish, German, Portuguese, and Valencian. */

export type CountKind =
  | "note"
  | "person"
  | "event"
  | "reminder"
  | "alarm"
  | "countdown"
  | "account"
  | "key"
  | "fact"
  | "transaction"
  | "schedule";

type Slot = "zero" | "one" | "two" | "few" | "many" | "other";

type Forms = Record<Slot, string>;

function forms(zero: string, one: string, other: string): Forms {
  return { zero, one, two: other, few: other, many: other, other };
}

const TABLES: Record<string, Record<CountKind, Forms>> = {
  en: {
    note: forms("{n} notes", "{n} note", "{n} notes"),
    person: forms("{n} people", "{n} person", "{n} people"),
    event: forms("{n} events", "{n} event", "{n} events"),
    reminder: forms("{n} reminders", "{n} reminder", "{n} reminders"),
    alarm: forms("{n} alarms", "{n} alarm", "{n} alarms"),
    countdown: forms("{n} countdowns", "{n} countdown", "{n} countdowns"),
    account: forms("{n} accounts", "{n} account", "{n} accounts"),
    key: forms("{n} keys", "{n} key", "{n} keys"),
    fact: forms("{n} pinned facts", "{n} pinned fact", "{n} pinned facts"),
    transaction: forms("{n} transactions", "{n} transaction", "{n} transactions"),
    schedule: forms("{n} schedules", "{n} schedule", "{n} schedules"),
  },
  es: {
    note: forms("{n} notas", "{n} nota", "{n} notas"),
    person: forms("{n} personas", "{n} persona", "{n} personas"),
    event: forms("{n} eventos", "{n} evento", "{n} eventos"),
    reminder: forms("{n} recordatorios", "{n} recordatorio", "{n} recordatorios"),
    alarm: forms("{n} alarmas", "{n} alarma", "{n} alarmas"),
    countdown: forms("{n} cuentas atrás", "{n} cuenta atrás", "{n} cuentas atrás"),
    account: forms("{n} cuentas", "{n} cuenta", "{n} cuentas"),
    key: forms("{n} claves", "{n} clave", "{n} claves"),
    fact: forms("{n} datos fijados", "{n} dato fijado", "{n} datos fijados"),
    transaction: forms("{n} movimientos", "{n} movimiento", "{n} movimientos"),
    schedule: forms("{n} programaciones", "{n} programación", "{n} programaciones"),
  },
  de: {
    note: forms("{n} Notizen", "{n} Notiz", "{n} Notizen"),
    person: forms("{n} Personen", "{n} Person", "{n} Personen"),
    event: forms("{n} Termine", "{n} Termin", "{n} Termine"),
    reminder: forms("{n} Erinnerungen", "{n} Erinnerung", "{n} Erinnerungen"),
    alarm: forms("{n} Wecker", "{n} Wecker", "{n} Wecker"),
    countdown: forms("{n} Countdowns", "{n} Countdown", "{n} Countdowns"),
    account: forms("{n} Konten", "{n} Konto", "{n} Konten"),
    key: forms("{n} Schlüssel", "{n} Schlüssel", "{n} Schlüssel"),
    fact: forms("{n} festgehaltene Fakten", "{n} festgehaltener Fakt", "{n} festgehaltene Fakten"),
    transaction: forms("{n} Buchungen", "{n} Buchung", "{n} Buchungen"),
    schedule: forms("{n} Pläne", "{n} Plan", "{n} Pläne"),
  },
  pt: {
    note: forms("{n} notas", "{n} nota", "{n} notas"),
    person: forms("{n} pessoas", "{n} pessoa", "{n} pessoas"),
    event: forms("{n} eventos", "{n} evento", "{n} eventos"),
    reminder: forms("{n} lembretes", "{n} lembrete", "{n} lembretes"),
    alarm: forms("{n} alarmes", "{n} alarme", "{n} alarmes"),
    countdown: forms("{n} contagens regressivas", "{n} contagem regressiva", "{n} contagens regressivas"),
    account: forms("{n} contas", "{n} conta", "{n} contas"),
    key: forms("{n} chaves", "{n} chave", "{n} chaves"),
    fact: forms("{n} dados fixados", "{n} dado fixado", "{n} dados fixados"),
    transaction: forms("{n} movimentos", "{n} movimento", "{n} movimentos"),
    schedule: forms("{n} programações", "{n} programação", "{n} programações"),
  },
  ca: {
    note: forms("{n} notes", "{n} nota", "{n} notes"),
    person: forms("{n} persones", "{n} persona", "{n} persones"),
    event: forms("{n} esdeveniments", "{n} esdeveniment", "{n} esdeveniments"),
    reminder: forms("{n} recordatoris", "{n} recordatori", "{n} recordatoris"),
    alarm: forms("{n} alarmes", "{n} alarma", "{n} alarmes"),
    countdown: forms("{n} comptes enrere", "{n} compte enrere", "{n} comptes enrere"),
    account: forms("{n} comptes", "{n} compte", "{n} comptes"),
    key: forms("{n} claus", "{n} clau", "{n} claus"),
    fact: forms("{n} dades fixades", "{n} dada fixada", "{n} dades fixades"),
    transaction: forms("{n} moviments", "{n} moviment", "{n} moviments"),
    schedule: forms("{n} programacions", "{n} programació", "{n} programacions"),
  },
};

export function pluralTag(locale: string): string {
  const tag = locale.trim().split("_").join("-").toLowerCase();
  if (tag.includes("valencia") || tag.startsWith("ca")) return "ca";
  if (tag.startsWith("es")) return "es";
  if (tag.startsWith("de")) return "de";
  if (tag.startsWith("pt")) return "pt";
  return "en";
}

export function countPhrase(locale: string, kind: CountKind, count: number): string {
  const tag = pluralTag(locale);
  const row = TABLES[tag][kind];
  const rules = new Intl.PluralRules(tag);
  const selected: Slot = count === 0 ? "zero" : (rules.select(count) as Slot);
  const template = row[selected] ?? row.other;
  return template.split("{n}").join(String(count));
}
