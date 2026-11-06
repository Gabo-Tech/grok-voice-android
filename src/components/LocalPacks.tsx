import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { api, errorText, type LocalPack, type PackProgress } from "@/lib/api";

function formatBytes(bytes: number) {
  const mb = bytes / (1024 * 1024);
  if (mb >= 10) return `${Math.round(mb)} MB`;
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

export function LocalPacks({ showHeading = true }: { showHeading?: boolean }) {
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

  const downloadMissing = async () => {
    for (const pack of packs.filter((pack) => !pack.ready)) {
      await download(pack.id);
    }
  };

  const redownloadReady = async () => {
    for (const pack of packs.filter((pack) => pack.ready)) {
      await download(pack.id);
    }
  };

  const removeReady = async () => {
    const readyPacks = packs.filter((pack) => pack.ready);
    for (const pack of readyPacks) {
      await remove(pack.id);
    }
  };

  const readyCount = packs.filter((pack) => pack.ready).length;
  const ready = packs.length > 0 && packs.every((pack) => pack.ready);
  const anyReady = packs.some((pack) => pack.ready);
  const anyMissing = packs.some((pack) => !pack.ready);
  const summary =
    packs.length === 0
      ? "Packs show up here once this screen can reach the phone."
      : ready
        ? "All packs are on this phone. Talk can use On device without a cloud key."
        : `${readyCount} of ${packs.length} packs are on this phone. Download the rest, or Talk uses the next cloud key.`;

  return (
    <div className="space-y-3">
      <div>
        {showHeading ? <p className="text-base text-foreground">On-device packs</p> : null}
        <p className="mt-1 text-sm leading-6 text-muted-foreground">{summary}</p>
      </div>
      <ul className="divide-y divide-border border-y border-border">
        {packs.map((pack) => {
          const current = progress?.id === pack.id ? progress : null;
          const ratio = current && current.total > 0 ? Math.min(100, Math.round((current.received / current.total) * 100)) : null;
          const size = pack.ready && pack.installedBytes > 0 ? formatBytes(pack.installedBytes) : formatBytes(pack.downloadBytes);
          return (
            <li key={pack.id} className="py-3">
              <div className="flex flex-wrap items-center gap-3">
                <div className="min-w-0 flex-1">
                  <p className="text-sm text-foreground">{pack.label}</p>
                  <p className="text-xs leading-5 text-muted-foreground">
                    {pack.detail} · {size}
                    {pack.ready ? " on device" : " download"}
                    {ratio !== null ? ` · ${formatBytes(current?.received ?? 0)} / ${formatBytes(current?.total ?? pack.downloadBytes)}` : ""}
                  </p>
                </div>
                {pack.ready ? (
                  <div className="flex shrink-0 gap-2">
                    <Button type="button" variant="outline" disabled={busy !== null} onClick={() => void download(pack.id)}>
                      {busy === pack.id ? "Downloading" : "Download again"}
                    </Button>
                    <Button type="button" variant="outline" disabled={busy !== null} onClick={() => void remove(pack.id)}>
                      Delete
                    </Button>
                  </div>
                ) : (
                  <Button type="button" disabled={busy !== null} onClick={() => void download(pack.id)}>
                    {busy === pack.id ? "Downloading" : "Download"}
                  </Button>
                )}
              </div>
              {ratio !== null && <Progress value={ratio} className="mt-2" />}
            </li>
          );
        })}
      </ul>
      <div className="flex gap-2">
        <Button type="button" variant="outline" disabled={busy !== null || !anyMissing} onClick={() => void downloadMissing()}>
          Download all
        </Button>
        <Button type="button" variant="outline" disabled={busy !== null || !anyReady} onClick={() => void redownloadReady()}>
          Re-download all
        </Button>
        <Button type="button" variant="outline" disabled={busy !== null || !anyReady} onClick={() => void removeReady()}>
          Delete all
        </Button>
      </div>
      {error && (
        <p role="alert" className="text-sm leading-6 text-foreground">
          {error} Check the connection and try this pack again. A cloud key still works without these files.
        </p>
      )}
    </div>
  );
}
