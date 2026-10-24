import { createContext, createElement, useContext, useState, type ReactNode } from "react";
import type { PresetId } from "./model";

export const RECENT_SOUNDS_KEY = "voiceweaver.recent-sounds.v1";
export const soundKey = (sound: PresetId) => `${sound.bank}:${sound.program}`;

function readRecentSounds(): PresetId[] {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(RECENT_SOUNDS_KEY) ?? "[]");
    if (!Array.isArray(stored)) return [];
    const seen = new Set<string>();
    return stored.filter((sound): sound is PresetId => {
      if (!sound || typeof sound !== "object" || !Number.isInteger(sound.bank) || sound.bank < 0 || sound.bank > 65535 ||
          !Number.isInteger(sound.program) || sound.program < 0 || sound.program > 255 || seen.has(soundKey(sound))) return false;
      seen.add(soundKey(sound));
      return true;
    }).slice(0, 5).map(({ bank, program }) => ({ bank, program }));
  } catch {
    return [];
  }
}

const RecentSoundsContext = createContext<{ recent: PresetId[]; remember: (sound: PresetId) => void } | null>(null);

export function RecentSoundsProvider({ children }: { children: ReactNode }) {
  const [recent, setRecent] = useState(readRecentSounds);
  function remember(sound: PresetId) {
    setRecent((previous) => {
      const next = [{ bank: sound.bank, program: sound.program }, ...previous.filter((entry) => soundKey(entry) !== soundKey(sound))].slice(0, 5);
      try { localStorage.setItem(RECENT_SOUNDS_KEY, JSON.stringify(next)); } catch { /* Keep history for this session when storage is unavailable. */ }
      return next;
    });
  }
  return createElement(RecentSoundsContext.Provider, { value: { recent, remember } }, children);
}

export function useRecentSounds() {
  const context = useContext(RecentSoundsContext);
  if (!context) throw new Error("SoundPicker must be inside RecentSoundsProvider");
  return context;
}
