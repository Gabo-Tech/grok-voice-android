import { useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { safeHttps } from "@/lib/links";
import { Trash2 } from "lucide-react";
import { EmptyState, Notice, PageChrome, RowButton } from "@/components/PageChrome";
import { ShareButton } from "@/components/ShareButton";
import { Sheet } from "@/components/Sheet";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import type { AgendaState } from "@/hooks/useAgenda";
import { formatWhen } from "@/hooks/useAgenda";
import { api, type Note } from "@/lib/api";
import { useT } from "@/lib/i18n";

export function NotesPage({ agenda, active = true }: { agenda: AgendaState; active?: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<Note | null>(null);
  const [title, setTitle] = useState("");
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  const [preview, setPreview] = useState(false);

  const openNew = () => {
    setEditing(null);
    setTitle("");
    setText("");
    setPreview(false);
    setOpen(true);
  };

  const openEdit = (note: Note) => {
    setEditing(note);
    setTitle(note.title || headingFrom(note.text));
    setText(note.text);
    setPreview(false);
    setOpen(true);
  };

  const close = () => {
    setOpen(false);
    setEditing(null);
    setTitle("");
    setText("");
  };

  const save = () => {
    if (!title.trim() && !text.trim()) return;
    void agenda.run(async () => {
      const snap = await api.saveNote(text, editing?.id, title);
      close();
      return snap;
    });
  };

  const remove = () => {
    if (!editing) return;
    void agenda.run(async () => {
      const snap = await api.deleteNote(editing.id);
      close();
      return snap;
    });
  };

  const notes = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return [...agenda.agenda.notes].reverse().filter((note) => {
      if (!needle) return true;
      return `${note.title ?? ""} ${note.text}`.toLowerCase().includes(needle);
    });
  }, [agenda.agenda.notes, query]);

  return (
    <>
      <PageChrome title={t("Notes")} subtitle={`${agenda.agenda.notes.length} ${t("saved")}`} onAdd={openNew} addLabel={t("New note")} active={active}>
        {(agenda.agenda.notes.length > 0 || query) && (
        <div className="mb-6">
          <Field label={t("Search")}>
            <div className="relative">
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("A word from a note")}
                aria-label={t("Search notes")}
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
        {agenda.error && <Notice title={t(agenda.error)} />}
        {notes.length === 0 ? (
          <EmptyState
            title={query ? t("No matches") : t("No notes yet")}
            body={query ? t("Try another word.") : t("Tap + or ask Grok to remember something.")}
            actionLabel={query ? t("Clear search") : t("New note")}
            onAction={query ? () => setQuery("") : openNew}
          />
        ) : (
          <div className="divide-y divide-border border-y border-border">
            {notes.map((note) => {
              const name = note.title || headingFrom(note.text) || t("Untitled");
              const previewLine = note.text.replace(/\s+/g, " ").trim();
              return (
                <RowButton key={note.id} onClick={() => openEdit(note)}>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-base leading-6 text-foreground">{name}</p>
                    <p className="mt-1 truncate text-sm leading-5 text-muted-foreground">
                      {formatWhen(note.createdMs)}
                      {previewLine ? ` · ${previewLine}` : ""}
                    </p>
                  </div>
                </RowButton>
              );
            })}
          </div>
        )}
      </PageChrome>

      <Sheet
        open={open}
        title={editing ? t("Edit note") : t("New note")}
        onClose={close}
        footer={
          <div className="flex flex-wrap items-center gap-2">
            <ShareButton title={title} text={text} />
            {editing && (
              <Button variant="ghost" disabled={agenda.busy} onClick={remove}>
                <Trash2 /> {t("Delete")}
              </Button>
            )}
            <div className="flex-1" />
            <Button variant="ghost" onClick={close}>
              {t("Cancel")}
            </Button>
            <Button disabled={agenda.busy || (!title.trim() && !text.trim())} onClick={save}>
              {t("Save")}
            </Button>
          </div>
        }
      >
        <div className="border border-border">
          <div className="px-3 pt-3">
            <Field label={t("Title")} error={open ? (agenda.error ? t(agenda.error) : null) : null}>
              <Input
                autoFocus
                value={title}
                aria-invalid={Boolean(agenda.error) || undefined}
                onChange={(e) => setTitle(e.target.value)}
                placeholder={t("Grocery list")}
                className="border-0 px-0 focus-visible:border-transparent"
              />
            </Field>
          </div>
          <div className="border-t border-border px-3 py-3">
            <div className="mb-2 flex items-center justify-between gap-3">
              <span className="text-sm text-muted-foreground">{t("Note")}</span>
              <button type="button" className="min-h-12 px-2 text-sm text-foreground" onClick={() => setPreview((on) => !on)}>
                {preview ? t("Edit") : t("Formatted")}
              </button>
            </div>
            {preview ? (
              <div className="min-h-[40vh] py-1">
                <NoteMarkdown text={text || t("Nothing written yet.")} />
              </div>
            ) : (
              <Textarea
                value={text}
                onChange={(e) => setText(e.target.value)}
                placeholder={t("- milk\n- **bread**")}
                className="min-h-[40vh] border-0 px-0 font-mono focus-visible:border-transparent"
              />
            )}
          </div>
        </div>
      </Sheet>
    </>
  );
}

function headingFrom(text: string): string {
  const line = text.split("\n").find((row) => row.trim().length > 0) ?? "";
  return line.replace(/^#+\s*/, "").trim();
}

function NoteMarkdown({ text }: { text: string }) {
  const blocks = text.trim().split(/\n{2,}/);
  return (
    <div className="space-y-3 text-base leading-6 text-foreground">
      {blocks.map((block, index) => {
        const lines = block.split("\n");
        if (lines.every((line) => /^\s*[-*]\s+/.test(line))) {
          return (
            <ul key={index} className="list-disc space-y-1 pl-5">
              {lines.map((line, i) => (
                <li key={i}>{inline(line.replace(/^\s*[-*]\s+/, ""))}</li>
              ))}
            </ul>
          );
        }
        const heading = /^(#{1,3})\s+(.*)$/.exec(lines[0] ?? "");
        if (heading && lines.length === 1) {
          const size = heading[1].length === 1 ? "text-xl" : "text-lg";
          return (
            <p key={index} className={`${size} font-medium`}>
              {inline(heading[2])}
            </p>
          );
        }
        return <p key={index}>{inline(block)}</p>;
      })}
    </div>
  );
}

function inline(value: string) {
  const parts = value.split(/(\*\*[^*]+\*\*|https?:\/\/[^\s)]+)/g);
  return parts.map((part, index) => {
    if (part.startsWith("**") && part.endsWith("**")) return <strong key={index}>{part.slice(2, -2)}</strong>;
    if (/^https?:\/\//.test(part) && safeHttps(part)) {
      return (
        <button
          key={index}
          type="button"
          className="break-all text-left underline"
          onClick={() => void openUrl(part)}
        >
          {part}
        </button>
      );
    }
    return <span key={index}>{part}</span>;
  });
}
