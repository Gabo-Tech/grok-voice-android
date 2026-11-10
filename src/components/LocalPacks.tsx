import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { api, errorText, type LocalPack, type PackProgress } from "@/lib/api";
import { useT } from "@/lib/i18n";

function formatBytes(bytes: number) {
  const mb = bytes / (1024 * 1024);
  if (mb >= 10) return `${Math.round(mb)} MB`;
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

function packBytes(pack: LocalPack) {
  const bytes = pack.ready && pack.installedBytes > 0 ? pack.installedBytes : pack.downloadBytes;
  return bytes > 0 ? formatBytes(bytes) : "";
}

const FAMILIES = [
  ["auto", "Auto"],
  ["chatml", "ChatML"],
  ["gemma", "Gemma"],
  ["llama3", "Llama 3"],
] as const;

export function LocalPacks({ showHeading = true, onChanged }: { showHeading?: boolean; onChanged?: () => void }) {
  const t = useT();
  const [packs, setPacks] = useState<LocalPack[]>([]);
  const [progress, setProgress] = useState<PackProgress | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [sha, setSha] = useState("");
  const [family, setFamily] = useState<(typeof FAMILIES)[number][0]>("auto");

  const refresh = () => {
    void api
      .localPacks()
      .then((next) => {
        setPacks(next);
        setError(null);
      })
      .catch((err) => setError(shownError(err, t)));
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
      setError(shownError(err, t));
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
      setError(shownError(err, t));
    } finally {
      setBusy(null);
    }
  };

  const useModel = async (id: string) => {
    setBusy(id);
    setError(null);
    try {
      setPacks(await api.selectLocalModel(id));
      onChanged?.();
    } catch (err) {
      setError(shownError(err, t));
    } finally {
      setBusy(null);
    }
  };

  const chooseFile = async () => {
    setBusy("pick");
    setError(null);
    try {
      setPacks(await api.importLocalModel(name.trim(), family));
      setName("");
      onChanged?.();
    } catch (err) {
      setError(shownError(err, t));
    } finally {
      setBusy(null);
    }
  };

  const addLink = async () => {
    setBusy("link");
    setError(null);
    setProgress(null);
    try {
      setPacks(await api.downloadCustomModel(url.trim(), sha.trim(), name.trim(), family));
      setName("");
      setUrl("");
      setSha("");
      onChanged?.();
    } catch (err) {
      setError(shownError(err, t));
    } finally {
      setBusy(null);
      setProgress(null);
    }
  };

  const speech = packs.filter((pack) => pack.kind !== "llm" && pack.kind !== "custom");
  const models = packs.filter((pack) => pack.kind === "llm" || pack.kind === "custom");
  const bulk = packs.filter((pack) => pack.kind === "speech" || pack.id === "llm" || (pack.kind === undefined && pack.id !== "llm-custom"));
  const needed = [...speech, ...models.filter((pack) => pack.selected)];
  const readyCount = needed.filter((pack) => pack.ready).length;
  const ready = needed.length > 0 && needed.every((pack) => pack.ready);
  const anyReady = bulk.some((pack) => pack.ready);
  const anyMissing = bulk.some((pack) => !pack.ready);
  const summary =
    packs.length === 0
      ? t("Packs show up here once this screen can reach the phone.")
      : ready
        ? t("All packs are on this phone. Talk can use On device without a cloud key.")
        : `${readyCount} / ${needed.length}. ${t("Download the ones that are missing, or Talk uses the next cloud key.")}`;

  const downloadMissing = async () => {
    for (const pack of bulk.filter((pack) => !pack.ready)) {
      await download(pack.id);
    }
  };

  const redownloadReady = async () => {
    for (const pack of bulk.filter((pack) => pack.ready)) {
      await download(pack.id);
    }
  };

  const removeReady = async () => {
    for (const pack of bulk.filter((pack) => pack.ready)) {
      await remove(pack.id);
    }
  };

  return (
    <div className="space-y-3">
      <div>
        {showHeading ? <p className="text-base text-foreground">{t("On-device packs")}</p> : null}
        <p className="mt-1 text-sm leading-6 text-muted-foreground">{summary}</p>
        <p className="mt-1 text-sm leading-6 text-muted-foreground">
          {t(
            "Speech to text is Moonshine Base. Choose a language model below. SmolLM2 135M Instruct stays the default until you switch. Spanish speech, German speech, and the Piper voices stay as they are.",
          )}
        </p>
      </div>
      <ul className="divide-y divide-border border-y border-border">
        {speech.map((pack) => (
          <PackRow key={pack.id} pack={pack} progress={progress} busy={busy} onDownload={download} onRemove={remove} />
        ))}
      </ul>
      <div className="flex flex-col gap-2">
        <Button type="button" variant="outline" className="w-full whitespace-normal" disabled={busy !== null || !anyMissing} onClick={() => void downloadMissing()}>
          {t("Download all")}
        </Button>
        <Button type="button" variant="outline" className="w-full whitespace-normal" disabled={busy !== null || !anyReady} onClick={() => void redownloadReady()}>
          {t("Re-download all")}
        </Button>
        <Button type="button" variant="outline" className="w-full whitespace-normal" disabled={busy !== null || !anyReady} onClick={() => void removeReady()}>
          {t("Delete all")}
        </Button>
      </div>

      <div className="pt-2">
        <p className="text-base text-foreground">{t("Language models")}</p>
        <p className="mt-1 text-sm leading-6 text-muted-foreground">
          {t(
            "Nothing new downloads until you tap Download. Talk loads the chosen file into memory, so a larger model needs more free storage and about that much free memory besides speech and voice.",
          )}
        </p>
      </div>
      <ul className="divide-y divide-border border-y border-border">
        {models.map((pack) => (
          <PackRow
            key={pack.id}
            pack={pack}
            progress={progress}
            busy={busy}
            onDownload={pack.kind === "custom" ? undefined : download}
            onRemove={remove}
            onUse={useModel}
          />
        ))}
      </ul>

      <div className="space-y-3 pt-2">
        <p className="text-base text-foreground">{t("Bring your own model")}</p>
        <p className="text-sm leading-6 text-muted-foreground">
          {t("One custom model can be saved. Adding another replaces it. A link must be https and at most 2 GB. The file is checked as a GGUF and is only loaded as the chat model.")}
        </p>
        <label className="block">
          <span className="mb-2 block text-sm leading-5 text-muted-foreground">{t("Model name")}</span>
          <Input value={name} maxLength={60} placeholder={t("Optional. Shown in this list.")} onChange={(event) => setName(event.target.value)} />
        </label>
        <div>
          <p className="mb-2 text-sm leading-5 text-muted-foreground">{t("Chat format")}</p>
          <div className="flex flex-wrap gap-2">
            {FAMILIES.map(([id, label]) => (
              <button
                key={id}
                type="button"
                aria-pressed={family === id}
                className={`h-12 border px-3 text-sm ${family === id ? "border-foreground text-foreground" : "border-border text-muted-foreground"}`}
                onClick={() => setFamily(id)}
              >
                {t(label)}
              </button>
            ))}
          </div>
        </div>
        <Button type="button" variant="outline" className="w-full whitespace-normal" disabled={busy !== null} onClick={() => void chooseFile()}>
          {busy === "pick" ? t("Copying") : t("Choose a GGUF")}
        </Button>
        <label className="block">
          <span className="mb-2 block text-sm leading-5 text-muted-foreground">{t("Download link")}</span>
          <Input
            value={url}
            inputMode="url"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            placeholder="https://…/model.gguf"
            onChange={(event) => setUrl(event.target.value)}
          />
        </label>
        <label className="block">
          <span className="mb-2 block text-sm leading-5 text-muted-foreground">{t("SHA-256")}</span>
          <Input
            value={sha}
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            placeholder={t("Optional. Leave blank to skip the checksum.")}
            onChange={(event) => setSha(event.target.value)}
          />
        </label>
        <Button type="button" className="w-full whitespace-normal" disabled={busy !== null || url.trim() === ""} onClick={() => void addLink()}>
          {busy === "link" ? t("Downloading") : t("Add from a link")}
        </Button>
      </div>

      {error && (
        <p role="alert" className="text-sm leading-6 text-foreground">
          {error}
        </p>
      )}
    </div>
  );
}

function shownError(err: unknown, t: (english: string) => string) {
  const text = t(errorText(err));
  const lower = text.toLowerCase();
  const network =
    lower.includes("download failed") ||
    lower.includes("checksum") ||
    lower.includes("https") ||
    lower.includes("address") ||
    lower.includes("connection") ||
    lower.includes("redirect");
  if (network) {
    return `${text} ${t("Check the connection and try this pack again. A cloud key still works without these files.")}`;
  }
  return text;
}

function PackRow({
  pack,
  progress,
  busy,
  onDownload,
  onRemove,
  onUse,
}: {
  pack: LocalPack;
  progress: PackProgress | null;
  busy: string | null;
  onDownload?: (id: string) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
  onUse?: (id: string) => Promise<void>;
}) {
  const t = useT();
  const current = progress?.id === pack.id ? progress : null;
  const ratio = current && current.total > 0 ? Math.min(100, Math.round((current.received / current.total) * 100)) : null;
  const size = packBytes(pack);
  const label = pack.kind === "custom" && pack.label !== "Custom model" ? pack.label : t(pack.label);
  return (
    <li className="space-y-3 py-3">
      <div className="min-w-0">
        <p className="text-base text-foreground">{label}</p>
        <p className="text-sm leading-5 text-muted-foreground">
          {t(pack.detail)}
          {size ? ` · ${size}` : ""} · {pack.ready ? t("On this phone") : t("Not downloaded")}
          {pack.selected ? ` · ${t("In use")}` : ""}
          {ratio !== null ? ` · ${formatBytes(current?.received ?? 0)} / ${formatBytes(current?.total ?? pack.downloadBytes)}` : ""}
        </p>
        {pack.note ? <p className="text-sm leading-5 text-muted-foreground">{t(pack.note)}</p> : null}
      </div>
      <div className="flex flex-col gap-2">
        {pack.ready && onUse && !pack.selected ? (
          <Button type="button" className="w-full whitespace-normal" disabled={busy !== null} onClick={() => void onUse(pack.id)}>
            {t("Use this model")}
          </Button>
        ) : null}
        {pack.ready && onDownload ? (
          <Button type="button" variant="outline" className="w-full whitespace-normal" disabled={busy !== null} onClick={() => void onDownload(pack.id)}>
            {busy === pack.id ? t("Downloading") : t("Download again")}
          </Button>
        ) : null}
        {!pack.ready && onDownload ? (
          <Button type="button" className="w-full whitespace-normal" disabled={busy !== null} onClick={() => void onDownload(pack.id)}>
            {busy === pack.id ? t("Downloading") : t("Download")}
          </Button>
        ) : null}
        {pack.ready ? (
          <Button type="button" variant="outline" className="w-full whitespace-normal" disabled={busy !== null} onClick={() => void onRemove(pack.id)}>
            {t("Delete")}
          </Button>
        ) : null}
      </div>
      {ratio !== null && <Progress value={ratio} className="mt-2" />}
    </li>
  );
}
