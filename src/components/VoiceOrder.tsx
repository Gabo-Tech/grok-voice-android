import { useEffect, useRef, useState, type PointerEvent } from "react";
import { GripVertical } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { VoiceAccount } from "@/lib/api";
import { chainPlace, providerLabel } from "@/lib/flow";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

function moveRow(rows: VoiceAccount[], from: number, to: number) {
  if (from === to || from < 0 || to < 0 || from >= rows.length || to >= rows.length) return rows;
  const next = rows.slice();
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}

export function VoiceOrder({
  accounts,
  onReorder,
  onEdit,
  onRemove,
}: {
  accounts: VoiceAccount[];
  onReorder: (ids: string[]) => Promise<void> | void;
  onEdit?: (id: string, provider: string) => void;
  onRemove: (id: string) => void;
}) {
  const t = useT();
  const [rows, setRows] = useState(accounts);
  const rowsRef = useRef(rows);
  const dragFrom = useRef<number | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const startIds = useRef<string[] | null>(null);
  const itemRefs = useRef<Array<HTMLLIElement | null>>([]);
  rowsRef.current = rows;

  useEffect(() => {
    setRows(accounts);
    rowsRef.current = accounts;
  }, [accounts]);

  async function commit(next: VoiceAccount[]) {
    const previous = accounts;
    setRows(next);
    rowsRef.current = next;
    try {
      await onReorder(next.map((account) => account.id));
    } catch {
      setRows(previous);
      rowsRef.current = previous;
    }
  }

  function onPointerDown(index: number, event: PointerEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragFrom.current = index;
    startIds.current = rowsRef.current.map((account) => account.id);
    setDragging(index);
  }

  function onPointerMove(event: PointerEvent<HTMLButtonElement>) {
    const from = dragFrom.current;
    if (from === null) return;
    const y = event.clientY;
    let hit = from;
    itemRefs.current.forEach((el, index) => {
      if (!el) return;
      const rect = el.getBoundingClientRect();
      if (y >= rect.top && y <= rect.bottom) hit = index;
    });
    if (hit === from) return;
    const next = moveRow(rowsRef.current, from, hit);
    rowsRef.current = next;
    dragFrom.current = hit;
    setDragging(hit);
    setRows(next);
  }

  function onPointerUp() {
    const from = dragFrom.current;
    const started = startIds.current;
    dragFrom.current = null;
    startIds.current = null;
    setDragging(null);
    if (from === null || !started) return;
    const ids = rowsRef.current.map((account) => account.id);
    if (ids.join("\n") === started.join("\n")) return;
    void commit(rowsRef.current);
  }

  function nudge(index: number, delta: number) {
    const next = moveRow(rows, index, index + delta);
    if (next !== rows) void commit(next);
  }

  if (rows.length === 0) return null;

  return (
    <div className="space-y-3">
      <p className="text-sm leading-6 text-muted-foreground">
        {t("Hold the handle and drag. The top voice is tried first. Each one under it is a fallback.")}
      </p>
      <ul className="divide-y divide-border border-y border-border">
        {rows.map((account, index) => (
          <li
            key={account.id}
            ref={(el) => {
              itemRefs.current[index] = el;
            }}
            className={cn("py-3", dragging === index && "border-foreground")}
          >
            <div className="flex items-start gap-2">
              <button
                type="button"
                className="grid size-12 shrink-0 touch-none place-items-center border border-border text-foreground"
                aria-label={t("Drag to reorder")}
                onPointerDown={(event) => onPointerDown(index, event)}
                onPointerMove={onPointerMove}
                onPointerUp={onPointerUp}
                onPointerCancel={onPointerUp}
                onKeyDown={(event) => {
                  if (event.key === "ArrowUp") {
                    event.preventDefault();
                    nudge(index, -1);
                  } else if (event.key === "ArrowDown") {
                    event.preventDefault();
                    nudge(index, 1);
                  }
                }}
              >
                <GripVertical className="size-5" aria-hidden />
              </button>
              <div className="min-w-0 flex-1">
                <p className="text-sm text-muted-foreground">{t(chainPlace(index, rows.length))}</p>
                <p className="mt-1 text-base text-foreground">
                  {t(providerLabel(account.provider))}
                  <span className="text-muted-foreground"> · {account.hint}</span>
                </p>
                <div className={cn("mt-2 grid gap-2", onEdit ? "grid-cols-2" : "grid-cols-1")}>
                  {onEdit && (
                    <Button type="button" variant="outline" onClick={() => onEdit(account.id, account.provider)}>
                      {t("Edit")}
                    </Button>
                  )}
                  <Button type="button" variant="outline" onClick={() => onRemove(account.id)}>
                    {t("Remove")}
                  </Button>
                </div>
              </div>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
