import { useState } from "react";
import { Button } from "@/components/ui/button";
import { shareText } from "@/lib/flow";
import { useT } from "@/lib/i18n";

export function ShareButton({ title, text, disabled }: { title: string; text: string; disabled?: boolean }) {
  const t = useT();
  const [message, setMessage] = useState("");
  return (
    <div className="flex flex-col items-start gap-1">
      <Button
        type="button"
        variant="outline"
        disabled={disabled || (!title.trim() && !text.trim())}
        onClick={() => {
          void shareText(title, text).then((result) => {
            if (result === "shared") setMessage(t("Nothing is sent until you confirm in the other app."));
            else if (result === "copied") setMessage(t("Copied. Paste it where you want it."));
            else if (result === "failed") setMessage(t("Couldn't copy that. Select the text above and copy it from there."));
            else setMessage("");
          });
        }}
      >
        {t("Share")}
      </Button>
      {message ? <p className="max-w-full text-sm leading-5 text-muted-foreground">{message}</p> : null}
    </div>
  );
}
