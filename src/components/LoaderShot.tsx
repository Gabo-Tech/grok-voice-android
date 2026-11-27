import { Loader } from "@/components/Loader";
import { Presence } from "@/components/Presence";
import { useT } from "@/lib/i18n";

/** Real loader layouts for the portrait, landscape, and text-zoom checks. */
export function LoaderShot({ which }: { which: string }) {
  const t = useT();
  if (which === "talk") {
    return (
      <div className="flex h-full min-h-0 flex-col px-4 pt-4">
        <h1 className="masthead text-[1.85rem]">{t("Talk")}</h1>
        <div className="talk-log mt-4">
          <div className="talk-bubble talk-bubble-user">
            <p>{t("Say “Hey Grok”, tap the microphone, or type when you can't.")}</p>
          </div>
          <div className="talk-bubble talk-bubble-grok">
            <p>{t("Connecting")}</p>
          </div>
        </div>
        <div className="talk-presence mt-auto">
          <Presence phase="connecting" onOpen={() => undefined} />
        </div>
      </div>
    );
  }
  if (which === "pack") {
    return (
      <main className="flex h-full min-w-0 flex-col px-5 py-6">
        <p className="text-[13px] tracking-[0.18em] text-muted-foreground uppercase">{t("Setup")}</p>
        <h1 className="masthead mt-2">{t("Downloading")}</h1>
        <p className="mt-3 text-base leading-6 text-muted-foreground">{t("Use Wi-Fi. If the download stops, what is already here stays, and setup continues from this screen.")}</p>
        <div className="mt-6">
          <Loader size="block" value={42} label={t("Downloading")} />
        </div>
      </main>
    );
  }
  if (which === "rates") {
    return (
      <main className="flex h-full flex-col px-5 py-6">
        <h1 className="masthead text-[1.85rem]">{t("Finance")}</h1>
        <div className="mt-6 max-w-full">
          <Loader size="block" label={t("Checking rates")} />
        </div>
      </main>
    );
  }
  if (which === "list") {
    return (
      <main className="flex h-full flex-col px-5 py-6">
        <h1 className="masthead text-[1.85rem]">{t("People")}</h1>
        <div className="mt-6">
          <Loader size="block" label={t("Looking up contacts")} />
          <p className="mt-3 text-sm leading-6 text-muted-foreground">{t("This stays on the phone.")}</p>
        </div>
      </main>
    );
  }
  return (
    <main className="grid h-full min-h-0">
      <Loader size="full" label={t("Opening Grok Voice")} />
    </main>
  );
}
