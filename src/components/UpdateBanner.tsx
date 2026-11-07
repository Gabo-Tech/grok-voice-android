import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/button";
import { api, type UpdateStatus } from "@/lib/api";
import { safeUpdateUrl } from "@/lib/links";
import { useT } from "@/lib/i18n";

export function UpdateBanner() {
  const inApp = isTauri();
  const t = useT();
  const [update, setUpdate] = useState<UpdateStatus | null>(null);
  const ticket = useRef(0);

  useEffect(() => {
    if (!inApp) return;
    let unlisten: (() => void) | undefined;
    const apply = (status: UpdateStatus, id: number) => {
      if (id !== ticket.current) return;
      setUpdate(status.version && safeUpdateUrl(status.url) ? status : null);
    };
    const id = ++ticket.current;
    void api.updateStatus().then((status) => apply(status, id)).catch(() => {});
    void listen<UpdateStatus>("voice://update", (ev) => {
      const next = ++ticket.current;
      apply(ev.payload, next);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [inApp]);

  if (!update) return null;

  const open = () => {
    if (!safeUpdateUrl(update.url)) return;
    void openUrl(update.url).catch(() => {});
  };

  const dismiss = () => {
    const id = ++ticket.current;
    setUpdate(null);
    void api
      .dismissUpdate()
      .then((status) => {
        if (id !== ticket.current) return;
        setUpdate(status.version && safeUpdateUrl(status.url) ? status : null);
      })
      .catch(() => {});
  };

  return (
    <div className="mx-4 mt-3 border border-border px-3 py-3" role="status">
      <p className="text-base leading-6 text-foreground">
        {t("A newer version is ready.")} {update.version}
      </p>
      <p className="mt-1 text-sm leading-6 text-muted-foreground">
        {t("It downloads from GitHub. Nothing installs until you open the file.")}
      </p>
      <div className="mt-3 flex items-center gap-2">
        <Button type="button" className="min-w-0 flex-1" onClick={open}>
          {t("Download update")}
        </Button>
        <button type="button" className="min-h-12 shrink-0 px-3 text-sm text-foreground" onClick={dismiss}>
          {t("Not now")}
        </button>
      </div>
    </div>
  );
}
