/** Same numbers as `src-tauri/src/local/fit.rs`. Keep the two together. */

export const EXTRA_HEADROOM_BYTES = 1_610_612_736;
export const MOONSHINE_FOOTPRINT = 250_807_309;
export const PIPER_FOOTPRINT = 67_097_098;
export const RESIDENT_HEADROOM = EXTRA_HEADROOM_BYTES + MOONSHINE_FOOTPRINT + PIPER_FOOTPRINT;

export type FitName = "fits" | "tight" | "unknown" | "no";

export interface FitCandidate {
  id: string;
  sizeBytes: number;
  minRamGb: number;
  strength: number;
}

const GIB = 1024 * 1024 * 1024;

export function totalGib(totalBytes: number): number {
  if (!totalBytes) return 0;
  return Math.floor((totalBytes + GIB / 2) / GIB);
}

function scaledFits(size: number, numer: number, denom: number, totalBytes: number): boolean {
  const scaled = Math.floor((size * numer) / denom);
  const withRuntime = Math.floor((scaled * 5) / 4);
  return withRuntime + RESIDENT_HEADROOM < totalBytes;
}

export function classify(totalBytes: number, sizeBytes: number, minRamGb: number): FitName {
  if (!totalBytes || !sizeBytes || !minRamGb) return "unknown";
  const total = totalGib(totalBytes);
  if (total >= minRamGb && scaledFits(sizeBytes, 1, 1, totalBytes)) return "fits";
  if (total >= Math.max(0, minRamGb - 1) && scaledFits(sizeBytes, 85, 100, totalBytes)) return "tight";
  return "no";
}

function rank(fit: FitName): number {
  if (fit === "fits") return 3;
  if (fit === "tight") return 2;
  if (fit === "unknown") return 1;
  return 0;
}

export function recommend(candidates: FitCandidate[], totalBytes: number): string | null {
  let best: { fit: FitName; strength: number; id: string } | null = null;
  for (const candidate of candidates) {
    const fit = classify(totalBytes, candidate.sizeBytes, candidate.minRamGb);
    if (fit === "no") continue;
    if (!best || rank(fit) > rank(best.fit) || (rank(fit) === rank(best.fit) && candidate.strength > best.strength)) {
      best = { fit, strength: candidate.strength, id: candidate.id };
    }
  }
  return best?.id ?? null;
}

export function fitLabel(fit: string | undefined): string {
  if (fit === "fits") return "Fits";
  if (fit === "tight") return "Tight";
  if (fit === "no") return "Does not fit";
  if (fit === "unknown") return "Fit unknown";
  return "";
}
