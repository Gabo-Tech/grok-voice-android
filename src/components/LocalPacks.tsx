import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { api, errorText, type LocalPack, type PackProgress } from "@/lib/api";

function formatBytes(bytes: number) {
  const mb = bytes / (1024 * 1024);
  if (mb >= 10) return `${Math.round(mb)} MB`;
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

export function LocalPacks() {
  const [packs, setPacks] = useState<LocalPack[]>([]);
  const [progress, setProgress] = useState<PackProgress | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => {
    void api
      .localPacks()
      .then((next) => {
        setPacks(next);
        setError(null);
      })
      .catch((err) => setError(errorText(err)));
  };

  useEffect(() => {
    if (!isTauri()) return;
    refresh();
    let unlisten: (() => void) | undefined;
    void listen<PackProgress>("voice://pack", (event) => setProgress(event.payload)).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const download = async (id: string) => {
    setBusy(id);
    setError(null);
    setProgress(null);
    try {
      setPacks(await api.downloadLocalPack(id));
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(null);
      setProgress(null);
    }
  };

  const remove = async (id: string) => {
    setBusy(id);
    setError(null);
    try {
      setPacks(await api.deleteLocalPack(id));
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(null);
    }
  };

  const ready = packs.length > 0 && packs.every((pack) => pack.ready);

  return (
    <div className="space-y-3">
      <div>
        <p className="text-sm text-foreground">On-device packs</p>
        <p className="mt-1 text-sm leading-6 text-muted-foreground">
          {ready
            ? "Speech, the language model, and the voice are on this phone. Talk can run without xAI."
            : "Download speech recognition, the language model, and the voice. They stay in app storage, not in the install."}
        </p>
      </div>
      <ul className="divide-y divide-border border-y border-border">
        {packs.map((pack) => {
          const current = progress?.id === pack.id ? progress : null;
          const ratio = current && current.total > 0 ? Math.min(100, Math.round((current.received / current.total) * 100)) : null;
          return (
            <li key={pack.id} className="flex items-center gap-3 py-3">
              <div className="min-w-0 flex-1">
                <p className="text-sm text-foreground">{pack.label}</p>
                <p className="text-xs leading-5 text-muted-foreground">
                  {pack.detail} · {formatBytes(pack.downloadBytes)}
                  {pack.ready ? " · ready" : ""}
                  {ratio !== null ? ` · ${ratio}%` : ""}
                </p>
              </div>
              {pack.ready ? (
                <Button type="button" variant="outline" disabled={busy !== null} onClick={() => void remove(pack.id)}>
                  Delete
                </Button>
              ) : (
                <Button type="button" disabled={busy !== null} onClick={() => void download(pack.id)}>
                  {busy === pack.id ? "Downloading" : "Download"}
                </Button>
              )}
            </li>
          );
        })}
      </ul>
      {error && <p className="text-sm text-foreground">{error}</p>}
    </div>
  );
}
