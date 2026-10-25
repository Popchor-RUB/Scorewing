export function advancePracticeProgress(value: number, completed: number, running: number, total: number, elapsedMs: number): number {
  if (total <= 0) return value;
  const completedPercent = 100 * completed / total;
  const ceiling = Math.min(99, 100 * (completed + running) / total);
  // Estimate each active track at ten seconds; only the export result confirms success.
  const estimated = value + 100 * running / total * elapsedMs / 10_000;
  return Math.max(value, Math.min(99, completedPercent), Math.min(ceiling, estimated));
}
