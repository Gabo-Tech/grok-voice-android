/** Built-in voices. Ids match the Rust normalizer and the cited docs. */

export type VoiceChoice = { id: string; tone?: string; accent?: string };

/** https://docs.x.ai/developers/model-capabilities/audio/voice — table order, Eve first in the picker. */
export const XAI_VOICES: VoiceChoice[] = [
  { id: "eve", tone: "Energetic and upbeat", accent: "British accent" },
  { id: "ara", tone: "Warm and friendly" },
  { id: "leo", tone: "Authoritative and strong", accent: "British accent" },
  { id: "rex", tone: "Confident and clear" },
  { id: "sal", tone: "Smooth and balanced" },
  { id: "carina", tone: "Soft, empathetic, and soothing" },
  { id: "zagan", tone: "Powerful, dramatic, and unmistakable" },
  { id: "helix", tone: "Bold, dynamic, and adrenaline-fueled" },
  { id: "orion", tone: "Rich, cinematic, and resonant" },
  { id: "luna", tone: "Gentle, patient, and deeply nurturing" },
  { id: "iris", tone: "Friendly, upbeat, and naturally charming" },
  { id: "altair", tone: "Elegant, refined, and effortlessly premium" },
  { id: "zenith", tone: "Sharp, focused, and driven" },
  { id: "perseus", tone: "Strong, confident, and trustworthy" },
  { id: "helios", tone: "Upbeat, energetic, and endlessly versatile" },
  { id: "lux", tone: "Grounded, calm, and quietly wise" },
  { id: "kepler", tone: "Inventive, forward-thinking, and charismatic" },
  { id: "rigel", tone: "Precise, professional, and calmly confident", accent: "Australian accent" },
  { id: "cosmo", tone: "Bright, curious, and easy to follow" },
  { id: "celeste", tone: "Compassionate, confident, and reassuring" },
  { id: "ursa", tone: "Friendly, warm, and steadfast" },
  { id: "sirius", tone: "Quick-witted, clever, and playful" },
  { id: "lumen", tone: "Warm, articulate, and engaging" },
  { id: "castor", tone: "Charismatic, down-to-earth, and easygoing" },
  { id: "naksh", tone: "Warm, thoughtful, and wise", accent: "Indian accent" },
  { id: "atlas", tone: "Confident, commanding, and reassuring" },
  { id: "aurora", tone: "Serene, steady, and radiant" },
  { id: "liora", tone: "Calm, grounded, and luminous" },
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
