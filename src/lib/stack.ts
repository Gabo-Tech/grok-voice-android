import type { OfflinePlan, PlanPiece } from "@/lib/api";

const STT = 250_807_309;
const LLM = 144_811_360;
const PHONON_EN = 65_051_089;

function piece(id: string, kind: string, bytes: number, label: string): PlanPiece {
  return { id, kind, bytes, label };
}

/** The first-run pack. Always English Moonshine, SmolLM2, and Phonon 31m. */
export function englishSetupPlan(): OfflinePlan {
  const pieces = [
    piece("stt", "pack", STT, "Speech to text"),
    piece("llm", "pack", LLM, "SmolLM2 135M Instruct"),
    piece("phonon-31m-en", "phonon", PHONON_EN, "Phonon 31m English"),
  ];
  return plan(
    "en",
    "en",
    "moonshine",
    "phonon",
    "phonon",
    "phonon-31m-en",
    false,
    "",
    pieces,
    null,
    "The offline pack is English: Moonshine, SmolLM2, and Phonon. No account and no token.",
  );
}

function plan(
  lang: string,
  uiLocale: string,
  hearing: string,
  voice: string,
  engine: string,
  phononModel: string,
  offerHf: boolean,
  hfModel: string,
  pieces: PlanPiece[],
  optional: PlanPiece | null,
  note: string,
): OfflinePlan {
  return {
    lang,
    uiLocale,
    hearing,
    brain: "smollm2",
    voice,
    engine,
    phononModel,
    offerHf,
    hfModel,
    pieces,
    bytes: pieces.reduce((sum, item) => sum + item.bytes, 0),
    note,
    optional,
  };
}

export function formatBytes(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb >= 10) return `${Math.round(mb)} MB`;
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  if (bytes <= 0) return "0 MB";
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
