import { classify, recommend, type FitCandidate, type FitName } from "./fit.ts";
import { englishSetupPlan, formatBytes } from "./stack.ts";

/** Piper English archive. The first-run voice is Phonon; Piper is the other on-device voice. */
export const PIPER_EN_BYTES = 67_097_098;

export type SetupPath = "local" | "xai" | "openai" | "onemin" | "mixed";
export type SetupScreen = "language" | "path" | SetupPath | "test";

export const PROGRESS = ["Language", "Choose", "Guide", "Try it"] as const;

export interface SetupChoice {
  id: SetupPath;
  title: string;
  blurb: string;
}

export const PATHS: SetupChoice[] = [
  {
    id: "local",
    title: "On this phone",
    blurb: "No key. Moonshine hears you, a local model thinks, and Phonon or Piper speaks.",
  },
  {
    id: "xai",
    title: "xAI",
    blurb: "A Grok key from xAI. Hearing, thinking, and the voice run in the cloud.",
  },
  {
    id: "openai",
    title: "OpenAI",
    blurb: "A key from OpenAI. Hearing, thinking, and the voice run in the cloud.",
  },
  {
    id: "onemin",
    title: "1min.ai",
    blurb: "A 1min.ai key for the reply. Hearing and the voice stay on this phone.",
  },
  {
    id: "mixed",
    title: "Phone, then a cloud key",
    blurb: "This phone answers first. A saved cloud key is used only if the phone cannot.",
  },
];

export interface KeyGuide {
  where: string;
  cost: string;
  save: string;
  test: string;
  error: string;
  short: string;
  url: string;
  placeholder: string;
}

export const KEY_GUIDES: Record<"xai" | "openai" | "onemin", KeyGuide> = {
  xai: {
    where: "Open the xAI console and create an API key. Copy the whole key. It usually starts with xai-.",
    cost: "xAI bills that key. This app does not charge you. Settings can warn you after a number of minutes.",
    save: "Paste the key, then save it. It stays in the vault on this phone.",
    test: "Save the key, allow the microphone, then open Talk and say hello. A spoken reply means the key works.",
    error: "If the key is refused, create a new one and check that the xAI account can spend. If the network fails, try again, or add On this phone as the next voice.",
    short: "The key is too short. Paste the whole key from the xAI console.",
    url: "https://console.x.ai/team/default/api-keys",
    placeholder: "xai-…",
  },
  openai: {
    where: "Open the OpenAI platform and create a secret key. Copy the whole key. It usually starts with sk-.",
    cost: "OpenAI bills that key. Realtime voice is their audio API. This app does not add a fee. Settings can warn you after a number of minutes.",
    save: "Paste the key, then save it. It stays in the vault on this phone.",
    test: "Save the key, allow the microphone, then open Talk and say hello. A spoken reply means the key works.",
    error: "If the key is refused, create a new one and check billing on the OpenAI platform. If the network fails, try again, or add On this phone as the next voice.",
    short: "The key is too short. Paste the whole key from the OpenAI platform.",
    url: "https://platform.openai.com/api-keys",
    placeholder: "sk-…",
  },
  onemin: {
    where: "Sign in at 1min.ai, open Settings, then API, and copy the key.",
    cost: "1min.ai spends credits on your account for the reply. Hearing and the voice stay on this phone, so they do not use those credits. This app does not add a fee.",
    save: "Paste the key, then save it. It stays in the vault on this phone.",
    test: "Save the key and download the English hearing and voice packs. Then open Talk and say hello. You are heard on this phone, and 1min.ai writes the reply.",
    error: "If the key is refused, copy it again from 1min.ai and check that the account has credits. If the reply never starts, download Moonshine and the voice pack, then try Talk again.",
    short: "The key is too short. Paste the whole key from 1min.ai.",
    url: "https://app.1min.ai",
    placeholder: "",
  },
};

/** Same pack sizes and minimum RAM as the on-device list. Strength matches that list. */
export function localModelCandidates(): FitCandidate[] {
  return [
    { id: "llm", sizeBytes: 144_811_360, minRamGb: 2, strength: 1 },
    { id: "llm-lfm25-350m", sizeBytes: 379_217_632, minRamGb: 2, strength: 2 },
    { id: "llm-qwen25-05b", sizeBytes: 675_710_816, minRamGb: 3, strength: 3 },
    { id: "llm-qwen35-08b", sizeBytes: 811_843_840, minRamGb: 2, strength: 4 },
    { id: "llm-llama32-1b", sizeBytes: 807_694_464, minRamGb: 3, strength: 5 },
    { id: "llm-gemma3-1b", sizeBytes: 806_058_240, minRamGb: 3, strength: 6 },
    { id: "llm-qwen35-2b", sizeBytes: 1_213_300_992, minRamGb: 3, strength: 7 },
    { id: "llm-gemma4-e2b", sizeBytes: 2_620_370_976, minRamGb: 8, strength: 8 },
    { id: "llm-qwen35-4b", sizeBytes: 2_708_804_608, minRamGb: 8, strength: 9 },
  ];
}

const MODEL_LABEL: Record<string, string> = {
  llm: "SmolLM2",
  "llm-lfm25-350m": "LFM2.5 350M",
  "llm-qwen25-05b": "Qwen2.5 0.5B",
  "llm-qwen35-08b": "Qwen3.5 0.8B",
  "llm-llama32-1b": "Llama 3.2 1B",
  "llm-gemma3-1b": "Gemma 3 1B",
  "llm-qwen35-2b": "Qwen3.5 2B",
  "llm-gemma4-e2b": "Gemma 4 E2B",
  "llm-qwen35-4b": "Qwen3.5 4B",
};

export interface LocalAdvice {
  modelId: string | null;
  modelLabel: string;
  ramGb: number;
  fit: FitName | "";
  storageBytes: number;
  piperBytes: number;
  knownMemory: boolean;
}

export function adviceFor(totalBytes: number): LocalAdvice {
  const storageBytes = englishSetupPlan().bytes;
  const base = {
    storageBytes,
    piperBytes: PIPER_EN_BYTES,
    modelLabel: "SmolLM2",
    ramGb: 2,
    fit: "" as FitName | "",
    modelId: null as string | null,
    knownMemory: totalBytes > 0,
  };
  if (!totalBytes) return base;
  const candidates = localModelCandidates();
  const modelId = recommend(candidates, totalBytes);
  if (!modelId) return base;
  const picked = candidates.find((item) => item.id === modelId);
  return {
    ...base,
    modelId,
    modelLabel: MODEL_LABEL[modelId] || "SmolLM2",
    ramGb: picked?.minRamGb ?? 2,
    fit: picked ? classify(totalBytes, picked.sizeBytes, picked.minRamGb) : "",
  };
}

/** `ram` query is a dev screenshot override. A phone uses navigator.deviceMemory when the browser reports it. */
export function reportedPhoneBytes(): number {
  if (typeof window !== "undefined" && import.meta.env.DEV) {
    const ram = Number(new URLSearchParams(window.location.search).get("ram"));
    if (ram > 0 && ram <= 64) return Math.round(ram * 1024 * 1024 * 1024);
  }
  if (typeof navigator === "undefined") return 0;
  const mem = (navigator as Navigator & { deviceMemory?: number }).deviceMemory;
  if (!mem || !Number.isFinite(mem) || mem <= 0) return 0;
  return Math.round(mem * 1024 * 1024 * 1024);
}

export function screenFor(raw: string | undefined): SetupScreen {
  switch ((raw || "").trim()) {
    case "path":
    case "cloud":
      return "path";
    case "local":
    case "packs":
    case "packs-run":
    case "error":
      return "local";
    case "xai":
    case "openai":
    case "onemin":
    case "mixed":
      return (raw || "").trim() as SetupPath;
    case "preview":
    case "test":
      return "test";
    default:
      return "language";
  }
}

export function progressIndex(screen: SetupScreen): number {
  if (screen === "language") return 0;
  if (screen === "path") return 1;
  if (screen === "test") return 3;
  return 2;
}

export function pathFromScreen(screen: SetupScreen): SetupPath {
  if (screen === "xai" || screen === "openai" || screen === "onemin" || screen === "mixed" || screen === "local") return screen;
  return "local";
}

export function isSetupPath(value: string): value is SetupPath {
  return value === "local" || value === "xai" || value === "openai" || value === "onemin" || value === "mixed";
}

export function memorySentence(advice: LocalAdvice): { template: string; pairs: Record<string, string> } {
  const sized = formatAdvice(advice);
  if (!advice.knownMemory) {
    return {
      template: "This phone did not report its memory. SmolLM2 135M is the safe start, and it needs about 2 GB. Voice and AI offers a larger model when one fits.",
      pairs: {},
    };
  }
  if (advice.modelId && advice.modelId !== "llm") {
    return {
      template: "{model} is recommended for this phone ({fit}). It needs about {ram} GB. The first download is still SmolLM2, about {size}, so Talk can answer before that larger file. You can switch in Voice and AI.",
      pairs: { model: advice.modelLabel, fit: advice.fit === "fits" ? "Fits" : advice.fit === "tight" ? "Tight" : "Fit unknown", ram: sized.ram, size: sized.storage },
    };
  }
  return {
    template: "SmolLM2 is the model this phone should run. It needs about {ram} GB. The download is about {size}.",
    pairs: { ram: sized.ram, size: sized.storage },
  };
}

export function formatAdvice(advice: LocalAdvice): { storage: string; piper: string; ram: string } {
  return {
    storage: formatBytes(advice.storageBytes),
    piper: formatBytes(advice.piperBytes),
    ram: String(advice.ramGb),
  };
}

/** English strings this guide introduces. Each one must exist in setup-i18n.json. */
export function guideKeys(): string[] {
  const keys = [
    "How should Talk answer?",
    "Pick one path. You can add the others later in Voice and AI.",
    "No key. Moonshine hears you, a local model thinks, and Phonon or Piper speaks.",
    "A Grok key from xAI. Hearing, thinking, and the voice run in the cloud.",
    "A key from OpenAI. Hearing, thinking, and the voice run in the cloud.",
    "A 1min.ai key for the reply. Hearing and the voice stay on this phone.",
    "Phone, then a cloud key",
    "This phone answers first. A saved cloud key is used only if the phone cannot.",
    "Get the key",
    "What it needs",
    "Save the key",
    "Test it",
    "Where to get it",
    "Open the key page",
    "Test this setup",
    "Continue setup",
    "Set up again",
    "Choose",
    "Guide",
    "Try it",
    "Skip this key",
    "Key saved on this phone.",
    "Which cloud key",
    "Cloud fallback",
    "Hearing and voice",
    "This sets the screen language. You can change it later in Settings. The first offline voice is English until you add another in Voice and AI.",
    "Come back to this screen from Settings, Voice and AI, whenever you want.",
    "Each path explains where to get a key, what it costs or needs, how to test it, and what to do if it fails.",
    "Setup can wait. Open Settings, then Voice and AI, and continue from the same path.",
    "A reply in Talk means this path works. You can change it later in Voice and AI.",
    "Check the key in Voice and AI. Create a new one if it was refused, then try Talk again.",
    "Nothing to buy. The files download into this phone. No account and no token.",
    "Moonshine turns speech into text. llama.cpp runs the chat model. Phonon or Piper speaks.",
    "The first download is about {size}. It is Moonshine, SmolLM2, and Phonon.",
    "Piper English is a separate file of about {piper}. Phonon speaks first and needs no account. You can switch to Piper in Voice and AI.",
    "SmolLM2 is the model this phone should run. It needs about {ram} GB. The download is about {size}.",
    "{model} is recommended for this phone ({fit}). It needs about {ram} GB. The first download is still SmolLM2, about {size}, so Talk can answer before that larger file. You can switch in Voice and AI.",
    "This phone did not report its memory. SmolLM2 135M is the safe start, and it needs about 2 GB. Voice and AI offers a larger model when one fits.",
    "Use Wi-Fi. If the download stops, what is already here stays, and setup continues from this screen.",
    "Play the voice. Then open Talk and say hello with the network off. A reply means the phone can answer alone.",
    "If the download stops, try it again. What is already on the phone stays. If a model does not fit, keep SmolLM2 or add a cloud key. If there is no reply, check that Moonshine, the chat model, and Phonon or Piper are on this phone.",
    "Memory",
    "Storage",
    "Two parts. The phone answers when it can. A cloud key answers when the phone cannot.",
    "Download the offline pack first. Then save one cloud key: xAI, OpenAI, or 1min.ai.",
    "Talk tries this phone first when the packs are ready. If a pack is missing, the model will not load, or memory is low, Talk uses the cloud key.",
    "Play the voice with the network off. Then turn the network on and say hello again. If the packs are missing, the cloud key should answer instead.",
    "If the phone is silent, finish the download. If the cloud key is silent, check that key and the network. The order in Voice and AI is the order Talk uses.",
    "1min.ai still needs this phone to hear you and to speak. Download the offline pack before the first talk.",
  ];
  for (const guide of Object.values(KEY_GUIDES)) {
    keys.push(guide.where, guide.cost, guide.save, guide.test, guide.error, guide.short);
  }
  return [...new Set(keys)];
}
