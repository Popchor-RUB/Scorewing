import type { PracticeDefaults } from "./model.ts";

export const PRACTICE_DEFAULTS_KEY = "voiceweaver.practice-defaults.v1";

function validDefaults(value: unknown): PracticeDefaults {
  const defaults: PracticeDefaults = {};
  if (!value || typeof value !== "object" || Array.isArray(value)) return defaults;
  const stored = value as Record<string, unknown>;
  for (const mode of ["foreground_sound", "background_sound"] as const) {
    const sound = stored[mode];
    if (sound && typeof sound === "object" && "bank" in sound && "program" in sound &&
        typeof sound.bank === "number" && Number.isInteger(sound.bank) && sound.bank >= 0 && sound.bank <= 65535 &&
        typeof sound.program === "number" && Number.isInteger(sound.program) && sound.program >= 0 && sound.program <= 255)
      defaults[mode] = { bank: sound.bank, program: sound.program };
  }
  for (const [field, min, max] of [["background_reduction_db", 0, 60], ["metronome_db", -60, 0]] as const) {
    const level = stored[field];
    if (typeof level === "number" && Number.isFinite(level) && level >= min && level <= max) defaults[field] = level;
  }
  if (typeof stored.metronome_enabled === "boolean") defaults.metronome_enabled = stored.metronome_enabled;
  return defaults;
}

export function readPracticeDefaults(): PracticeDefaults {
  try { return validDefaults(JSON.parse(localStorage.getItem(PRACTICE_DEFAULTS_KEY) ?? "{}")); }
  catch { return {}; }
}

export function savePracticeDefaults(defaults: PracticeDefaults): void {
  try { localStorage.setItem(PRACTICE_DEFAULTS_KEY, JSON.stringify(validDefaults(defaults))); }
  catch { /* Keep defaults for this session when storage is unavailable. */ }
}
