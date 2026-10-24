export const RECENT_SCORES_KEY = "voiceweaver.recent-scores.v1";

export function readRecentScores(): string[] {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(RECENT_SCORES_KEY) ?? "[]");
    if (!Array.isArray(stored)) return [];
    return [...new Set(stored.filter((path): path is string => typeof path === "string" && path.trim().length > 0))].slice(0, 10);
  } catch {
    return [];
  }
}

export function rememberScore(previous: string[], path: string): string[] {
  const next = [path, ...previous.filter((entry) => entry !== path)].slice(0, 10);
  try { localStorage.setItem(RECENT_SCORES_KEY, JSON.stringify(next)); }
  catch { /* Keep history for this session when storage is unavailable. */ }
  return next;
}
