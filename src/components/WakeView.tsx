import { useState } from "react";
import { CheckCircle2, Circle, Loader2, Mic, RotateCcw, Wand2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { api, errorText, type WakewordStatus } from "@/lib/api";

const SECONDS = 2.2;

export function WakeView({
  status,
  onStatus,
}: {
  status: WakewordStatus;
  onStatus: (s: WakewordStatus) => void;
}) {
  const [recording, setRecording] = useState<number | null>(null);
  const [progress, setProgress] = useState(0);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const record = async (index: number) => {
    setMessage(null);
    setRecording(index);
    setProgress(0);
    const started = performance.now();
    const timer = window.setInterval(() => {
      setProgress(Math.min(100, ((performance.now() - started) / (SECONDS * 1000)) * 100));
    }, 50);
    try {
      const info = await api.recordWakeSample(index, SECONDS);
      setMessage({ ok: true, text: `Sample ${index + 1} captured (${info.durationMs} ms of speech).` });
      onStatus(await api.wakewordStatus());
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    } finally {
      window.clearInterval(timer);
      setRecording(null);
      setProgress(0);
    }
  };

  const train = async () => {
    setBusy(true);
    setMessage(null);
    try {
      onStatus(await api.trainWakeword());
      setMessage({ ok: true, text: "“Hey Grok” enrolled. Turn on Listen on the Talk tab." });
    } catch (e) {
      setMessage({ ok: false, text: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  const reset = async () => {
    onStatus(await api.deleteWakeword());
    setMessage(null);
  };

  const allRecorded = status.samples.every(Boolean);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Wand2 className="size-4 text-primary" /> Enroll “Hey Grok”
        </CardTitle>
        <CardDescription>
          Record yourself saying “Hey Grok” three times, in a quiet room, holding the phone the
          way you normally would. The recordings stay on this phone.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {status.samples.map((done, i) => (
          <div key={i} className="flex items-center gap-3">
            {done ? (
              <CheckCircle2 className="size-5 text-primary" />
            ) : (
              <Circle className="size-5 text-muted-foreground" />
            )}
            <span className="flex-1 text-sm">Sample {i + 1}</span>
            <Button
              size="sm"
              variant={done ? "outline" : "default"}
              disabled={recording !== null || busy}
              onClick={() => void record(i)}
            >
              {recording === i ? <Loader2 className="animate-spin" /> : <Mic />}
              {recording === i ? "Speak now" : done ? "Redo" : "Record"}
            </Button>
          </div>
        ))}

        {recording !== null && <Progress value={progress} />}

        {message && (
          <p className={message.ok ? "text-sm text-primary" : "text-sm text-destructive"}>{message.text}</p>
        )}

        <div className="flex gap-2 pt-2">
          <Button className="flex-1" disabled={!allRecorded || busy || recording !== null} onClick={() => void train()}>
            {busy ? <Loader2 className="animate-spin" /> : <Wand2 />}
            {status.trained ? "Re-train" : "Train wake word"}
          </Button>
          <Button variant="outline" disabled={busy || recording !== null} onClick={() => void reset()}>
            <RotateCcw /> Reset
          </Button>
        </div>
        <p className="text-[11px] text-muted-foreground">
          Status: {status.trained ? "trained ✓" : "not trained"}. Too many false wakes? Raise the
          sensitivity threshold in Debug → Settings.
        </p>
      </CardContent>
    </Card>
  );
}
