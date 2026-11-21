/** Built-in voices. Ids match the Rust normalizer and the cited docs. */

export type VoiceChoice = { id: string; tone?: string; accent?: string };

/** https://docs.x.ai/developers/model-capabilities/audio/voice — table order, Eve first in the picker. */
export const XAI_VOICES: VoiceChoice[] = [
  { id: "eve", tone: "Energetic and upbeat", accent: "British accent" },
  { id: "ara", tone: "Warm and friendly" },
  { id: "leo", tone: "Authoritative and strong", accent: "British accent" },
  { id: "rex", tone: "Confident and clear" },
  { id: "sal", tone: "Smooth and balanced" },
  { id: "carina", tone: "Soft and empathetic" },
  { id: "zagan", tone: "Powerful and dramatic" },
  { id: "helix", tone: "Bold and dynamic" },
  { id: "orion", tone: "Rich and cinematic" },
  { id: "luna", tone: "Gentle and patient" },
  { id: "iris", tone: "Friendly and upbeat" },
  { id: "altair", tone: "Elegant and refined" },
  { id: "zenith", tone: "Sharp and focused" },
  { id: "perseus", tone: "Strong and confident" },
  { id: "helios", tone: "Upbeat and energetic" },
  { id: "lux", tone: "Grounded and calm" },
  { id: "kepler", tone: "Inventive and forward-thinking" },
  { id: "rigel", tone: "Precise and professional", accent: "Australian accent" },
  { id: "cosmo", tone: "Bright and curious" },
  { id: "celeste", tone: "Compassionate and confident" },
  { id: "ursa", tone: "Friendly and warm" },
  { id: "sirius", tone: "Quick-witted and clever" },
  { id: "lumen", tone: "Warm and articulate" },
  { id: "castor", tone: "Charismatic and down-to-earth" },
  { id: "naksh", tone: "Warm and thoughtful", accent: "Indian accent" },
  { id: "atlas", tone: "Confident and commanding" },
  { id: "aurora", tone: "Serene and steady" },
  { id: "liora", tone: "Calm and grounded" },
];

/** OpenAI Realtime built-in voices. Not the larger text-to-speech list. */
export const OPENAI_VOICES: VoiceChoice[] = [
  { id: "marin" },
  { id: "alloy" },
  { id: "ash" },
  { id: "ballad" },
  { id: "coral" },
  { id: "echo" },
  { id: "sage" },
  { id: "shimmer" },
  { id: "verse" },
  { id: "cedar" },
];

export function voiceLabel(id: string): string {
  if (!id) return "";
  return id.charAt(0).toUpperCase() + id.slice(1);
}

export function xaiSampleUrl(id: string): string {
  return `https://docs.x.ai/assets/audio/voice_${id}.mp3`;
}
