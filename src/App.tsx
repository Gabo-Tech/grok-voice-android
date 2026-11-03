import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { Bug, CalendarClock, Loader2, Radio, Wand2 } from "lucide-react";
import { AgendaView } from "@/components/AgendaView";
import { AssistantView } from "@/components/AssistantView";
import { DebugPanel } from "@/components/DebugPanel";
import { Onboarding } from "@/components/Onboarding";
import { WakeView } from "@/components/WakeView";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useVoice } from "@/hooks/useVoice";
import { api, errorText, type AuthStatus, type WakewordStatus } from "@/lib/api";

const PREVIEW_AUTH: AuthStatus = { hasKey: false, keyHint: null, securityLevel: null };
const EMPTY_WAKE: WakewordStatus = { trained: false, samples: [false, false, false] };

export default function App() {
  const inApp = isTauri();
  const [auth, setAuth] = useState<AuthStatus | null>(inApp ? null : PREVIEW_AUTH);
  const [onboarded, setOnboarded] = useState<boolean | null>(inApp ? null : false);
  const [bootError, setBootError] = useState<string | null>(null);
  const [wake, setWake] = useState<WakewordStatus>(EMPTY_WAKE);
  const voice = useVoice();

  useEffect(() => {
    if (!inApp) return;
    api.authStatus().then(setAuth).catch((e) => setBootError(errorText(e)));
    void api.wakewordStatus().then(setWake);
    api
      .getSettings()
      .then((s) => setOnboarded(s.onboarded))
      .catch(() => setOnboarded(false));
  }, [inApp]);

  if (bootError) {
    return (
      <main className="grid h-full place-items-center p-6 text-center text-sm text-destructive">
        Secure storage failed to open: {bootError}
      </main>
    );
  }

  if (!auth || onboarded === null) {
    return (
      <main className="grid h-full place-items-center">
        <Loader2 className="size-6 animate-spin text-primary" />
      </main>
    );
  }

  if (!onboarded || !auth.hasKey) {
    return (
      <main className="safe-area h-full">
        <Onboarding
          preview={!inApp}
          auth={auth}
          wake={wake}
          onAuth={setAuth}
          onWake={setWake}
          onFinished={() => setOnboarded(true)}
        />
      </main>
    );
  }

  return (
    <main className="safe-area flex h-full flex-col">
      <header className="flex items-center justify-between px-5 pt-4 pb-2">
        <h1 className="text-sm font-semibold tracking-[0.3em] text-primary">GROK//VOICE</h1>
        <Badge variant="outline" className="border-primary/30 text-[10px] text-muted-foreground">
          {auth.keyHint}
        </Badge>
      </header>

      <Tabs defaultValue="assistant" className="flex min-h-0 flex-1 flex-col px-4 pb-3">
        <TabsList className="grid w-full shrink-0 grid-cols-4">
          <TabsTrigger value="assistant" className="px-1 text-[11px]">
            <Radio /> Talk
          </TabsTrigger>
          <TabsTrigger value="plan" className="px-1 text-[11px]">
            <CalendarClock /> Plan
          </TabsTrigger>
          <TabsTrigger value="wake" className="px-1 text-[11px]">
            <Wand2 /> Wake
          </TabsTrigger>
          <TabsTrigger value="debug" className="px-1 text-[11px]">
            <Bug /> Debug
          </TabsTrigger>
        </TabsList>
        <TabsContent value="assistant" className="mt-3 flex min-h-0 flex-1 flex-col overflow-hidden">
          <AssistantView voice={voice} wakeTrained={wake.trained} />
        </TabsContent>
        <TabsContent value="plan" className="mt-3 min-h-0 flex-1 overflow-y-auto">
          <AgendaView />
        </TabsContent>
        <TabsContent value="wake" className="mt-3 min-h-0 flex-1 overflow-y-auto">
          <WakeView status={wake} onStatus={setWake} />
        </TabsContent>
        <TabsContent value="debug" className="mt-3 min-h-0 flex-1 overflow-y-auto">
          <DebugPanel
            voice={voice}
            onSignedOut={() => {
              setOnboarded(false);
              void api.authStatus().then(setAuth);
            }}
            onReplayTour={() => setOnboarded(false)}
          />
        </TabsContent>
      </Tabs>
    </main>
  );
}
