import { useState } from "react";
import { Trash2 } from "lucide-react";
import { EmptyState, PageChrome, RowButton } from "@/components/PageChrome";
import { Sheet } from "@/components/Sheet";
import { Button } from "@/components/ui/button";
import type { AgendaState } from "@/hooks/useAgenda";
import { formatWhen } from "@/hooks/useAgenda";
import { api, type Note } from "@/lib/api";

export function NotesPage({ agenda }: { agenda: AgendaState }) {
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<Note | null>(null);
  const [text, setText] = useState("");

  const openNew = () => {
    setEditing(null);
    setText("");
    setOpen(true);
  };

  const openEdit = (note: Note) => {
    setEditing(note);
    setText(note.text);
    setOpen(true);
  };

  const close = () => {
    setOpen(false);
    setEditing(null);
    setText("");
  };

  const save = () => {
    const next = text.trim();
    if (!next) return;
    void agenda.run(async () => {
      const snap = await api.saveNote(next, editing?.id);
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

  const notes = [...agenda.agenda.notes].reverse();

  return (
    <>
      <PageChrome title="Notes" subtitle={`${notes.length} saved`} onAdd={openNew} addLabel="New note">
        {agenda.error && <p className="mb-3 px-1 text-xs text-muted-foreground">{agenda.error}</p>}
        {notes.length === 0 ? (
          <EmptyState title="No notes yet" body="Tap + or ask Grok to remember something." />
        ) : (
          <div className="divide-y divide-border border-y border-border">
            {notes.map((note, i) => {
              const [head, ...rest] = note.text.split("\n");
              const preview = rest.join(" ").trim() || head;
              return (
                <RowButton
                  key={note.id}
                  onClick={() => openEdit(note)}
                  className="animate-row-in"
                  style={{ animationDelay: `${Math.min(i, 8) * 40}ms` } as React.CSSProperties}
                >
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-[15px] text-foreground">{head || "Untitled"}</p>
                    <p className="mt-1 truncate text-xs text-muted-foreground">
                      {formatWhen(note.createdMs)}
                      {preview && preview !== head ? ` · ${preview}` : ""}
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
        title={editing ? "Edit note" : "New note"}
        onClose={close}
        footer={
          <div className="flex items-center gap-2">
            {editing && (
              <Button variant="ghost" disabled={agenda.busy} onClick={remove}>
                <Trash2 /> Delete
              </Button>
            )}
            <div className="flex-1" />
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button disabled={agenda.busy || !text.trim()} onClick={save}>
              Save
            </Button>
          </div>
        }
      >
        <textarea
          autoFocus
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Write a note…"
          className="min-h-[40vh] w-full resize-none bg-transparent text-base leading-relaxed text-foreground outline-none placeholder:text-muted-foreground"
        />
      </Sheet>
    </>
  );
}
