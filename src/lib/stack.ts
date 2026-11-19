import type { OfflinePlan, PlanPiece } from "@/lib/api";

const STT = 250_807_309;
const LLM = 144_811_360;
const STT_ES = 107_246_685;
const TTS_ES = 67_184_952;
const STT_DE = 108_124_747;
const TTS_DE = 67_101_576;
const PHONON_EN = 65_051_089;
const PHONON_ES = 75_093_175;
const PHONON_DE = 75_092_449;
const PHONON_PT = 75_093_372;

function piece(id: string, kind: string, bytes: number, label: string): PlanPiece {
  return { id, kind, bytes, label };
}

/** Preview of the Rust plan when the phone bridge is not running. */
export function previewPlan(raw: string): OfflinePlan {
  const tag = raw.trim().toLowerCase().replace(/_/g, "-");
  const lang = tag.includes("valencia") || tag === "ca" || tag.startsWith("ca-")
    ? "ca"
    : tag.startsWith("pt")
      ? "pt"
      : tag.startsWith("de")
        ? "de"
        : tag.startsWith("es")
          ? "es"
          : "en";
  if (lang === "es") {
    const pieces = [
      piece("stt-es", "pack", STT_ES, "Spanish speech to text"),
      piece("llm", "pack", LLM, "SmolLM2 135M Instruct"),
      piece("tts-es", "pack", TTS_ES, "Spanish voice"),
    ];
    return plan(lang, "es", "nemo", "piper", "piper", "", true, "phonon-40m-es", pieces, piece("phonon-40m-es", "phonon", PHONON_ES, "Phonon 40m Spanish"), "Spanish and German listen on this phone and speak with Piper. A Hugging Face token adds Phonon.");
  }
  if (lang === "de") {
    const pieces = [
      piece("stt-de", "pack", STT_DE, "German speech to text"),
      piece("llm", "pack", LLM, "SmolLM2 135M Instruct"),
      piece("tts-de", "pack", TTS_DE, "German voice"),
    ];
    return plan(lang, "de", "nemo", "piper", "piper", "", true, "phonon-40m-de", pieces, piece("phonon-40m-de", "phonon", PHONON_DE, "Phonon 40m German"), "Spanish and German listen on this phone and speak with Piper. A Hugging Face token adds Phonon.");
  }
  if (lang === "pt") {
    return plan(lang, "pt", "none", "android", "auto", "", true, "phonon-40m-pt", [], piece("phonon-40m-pt", "phonon", PHONON_PT, "Phonon 40m Portuguese"), "Portuguese speaks with the Android voice until Phonon is downloaded with a Hugging Face token. Hearing you needs a cloud key.");
  }
  if (lang === "ca") {
    return plan(lang, "ca-ES-valencia", "none", "android", "android", "", false, "", [], null, "Valencian speaks with the Android voice. There is no Phonon model and no Piper voice. Hearing you needs a cloud key.");
  }
  const pieces = [
    piece("stt", "pack", STT, "Speech to text"),
    piece("llm", "pack", LLM, "SmolLM2 135M Instruct"),
    piece("phonon-31m-en", "phonon", PHONON_EN, "Phonon 31m English"),
  ];
  return plan("en", "en", "moonshine", "phonon", "phonon", "phonon-31m-en", false, "", pieces, null, "English uses Moonshine and Phonon. No account and no token.");
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
