import { useState } from "react";
import { Trash2 } from "lucide-react";
import { EmptyState, PageChrome, RowButton } from "@/components/PageChrome";
import { Sheet } from "@/components/Sheet";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { AgendaState } from "@/hooks/useAgenda";
import { formatWhen, fromLocalInput, toLocalInput } from "@/hooks/useAgenda";
import { api, type ScheduledItem } from "@/lib/api";

export function RemindersPage({ agenda }: { agenda: AgendaState }) {
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<ScheduledItem | null>(null);
  const [title, setTitle] = useState("");
  const [at, setAt] = useState(() => toLocalInput(Date.now() + 3_600_000));
  const [note, setNote] = useState("");

  const items = agenda.agenda.reminders;

  const openNew = () => {
    setEditing(null);
    setTitle("");
    setAt(toLocalInput(Date.now() + 3_600_000));
    setNote("");
    setOpen(true);
  };

  const openEdit = (item: ScheduledItem) => {
    setEditing(item);
    setTitle(item.title);
    setAt(toLocalInput(item.atMs));
    setNote(item.note || "");
    setOpen(true);
  };

  const close = () => {
    setOpen(false);
    setEditing(null);
  };

  const save = () => {
    const next = title.trim();
    if (!next) return;
    void agenda.run(async () => {
      const snap = await api.saveScheduled({
        title: next,
        atMs: fromLocalInput(at),
        note: note.trim(),
        kind: editing?.kind ?? "reminder",
        id: editing?.id,
      });
      close();
      return snap;
    });
  };

  const remove = () => {
    if (!editing) return;
    void agenda.run(async () => {
      const snap = await api.cancelScheduled(editing.id);
      close();
      return snap;
    });
  };

  return (
    <>
      <PageChrome title="Reminders" subtitle={`${items.length} upcoming`} onAdd={openNew} addLabel="New reminder">
        {agenda.error && <p className="mb-3 px-1 text-xs text-muted-foreground">{agenda.error}</p>}
        {items.length === 0 ? (
          <EmptyState title="Nothing scheduled" body="Tap + for a reminder, or ask Grok to nudge you later." />
        ) : (
          <div className="divide-y divide-border border-y border-border">
            {items.map((item, i) => (
              <RowButton
                key={item.id}
                onClick={() => openEdit(item)}
                className="animate-row-in gap-3"
                style={{ animationDelay: `${Math.min(i, 8) * 40}ms` } as React.CSSProperties}
              >
                <span className="mt-1 size-4 shrink-0 rounded-full border border-foreground/70" aria-hidden />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[15px] text-foreground">{item.title}</p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    <span className="uppercase tracking-wider">{item.kind}</span>
                    {" · "}
                    {formatWhen(item.atMs)}
                  </p>
                </div>
              </RowButton>
            ))}
          </div>
        )}
      </PageChrome>

      <Sheet
        open={open}
        title={editing ? "Edit reminder" : "New reminder"}
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
            <Button disabled={agenda.busy || !title.trim()} onClick={save}>
              Save
            </Button>
          </div>
        }
      >
        <div className="space-y-4">
          <label className="block space-y-1.5">
            <span className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">Title</span>
            <Input autoFocus value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Reminder" />
          </label>
          <label className="block space-y-1.5">
            <span className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">When</span>
            <Input type="datetime-local" value={at} onChange={(e) => setAt(e.target.value)} />
          </label>
          <label className="block space-y-1.5">
            <span className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">Note</span>
            <Input value={note} onChange={(e) => setNote(e.target.value)} placeholder="Optional" />
          </label>
        </div>
      </Sheet>
    </>
  );
}
