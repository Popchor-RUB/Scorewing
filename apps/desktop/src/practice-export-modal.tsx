import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "./components/ui/button.tsx";
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogTitle } from "./components/ui/dialog.tsx";
import type { PracticeProgress, PracticeResult } from "./model.ts";
import { advancePracticeProgress } from "./practice-progress.ts";

function PracticeProgressContent({ progress, total: selectedTotal, cancelling, onCancel }: {
  progress?: PracticeProgress;
  total: number;
  cancelling: boolean;
  onCancel: () => void;
}) {
  const { completed = 0, running = 0, total = selectedTotal } = progress ?? {};
  const [percentage, setPercentage] = useState(0);
  const value = advancePracticeProgress(percentage, completed, running, total, 0);

  useEffect(() => {
    setPercentage((value) => advancePracticeProgress(value, completed, running, total, 0));
    if (!running || cancelling) return;
    let previous = performance.now();
    const timer = window.setInterval(() => {
      const now = performance.now();
      const elapsed = now - previous;
      previous = now;
      setPercentage((value) => advancePracticeProgress(value, completed, running, total, elapsed));
    }, 100);
    return () => window.clearInterval(timer);
  }, [completed, running, total, cancelling]);

  return <>
    <DialogTitle>Exporting practice tracks</DialogTitle>
    <DialogDescription>Completed files are kept if rendering is cancelled.</DialogDescription>
    <div className="practice-progress dialog-body flex flex-col gap-4">
      <p role="status">{cancelling ? "Cancelling…" : progress?.stage ?? "Choose a folder for your practice tracks."} · {completed} / {total} tracks completed</p>
      <p className="muted">{running} {running === 1 ? "export" : "exports"} currently running</p>
      <progress className="practice-progress-bar" aria-label="Practice track rendering" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={value} max={100} value={value} />
    </div>
    <div className="dialog-footer">
      <Button variant="outline" disabled={cancelling} data-action="practice-cancel" onClick={onCancel}>{cancelling ? "Cancelling…" : "Cancel rendering"}</Button>
    </div>
  </>;
}

export function PracticeExportModal({ result, progress, total, cancelling, onCancel, onClose }: {
  result: PracticeResult | null;
  progress?: PracticeProgress;
  total: number;
  cancelling: boolean;
  onCancel: () => void;
  onClose: () => void;
}) {
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState("");
  const doneButton = useRef<HTMLButtonElement>(null);
  useEffect(() => { if (result) doneButton.current?.focus(); }, [result]);

  async function openFolder() {
    if (opening || !result) return;
    setOpening(true); setError("");
    try { await invoke("open_practice_export_folder", { directory: result.directory }); }
    catch (error) { setError(`Could not open the folder: ${String(error)}`); }
    finally { setOpening(false); }
  }

  return <Dialog open disablePointerDismissal={!result} onOpenChange={(open, details) => {
    if (!open && !result) { details.cancel(); return; }
    if (!open) onClose();
  }}>
    <DialogContent className="practice-export-dialog">
      {result ? <>
        <div className="dialog-header"><DialogTitle>{result.cancelled ? "Rendering cancelled" : result.error ? "Rendering stopped" : "Practice tracks created"}</DialogTitle>
          <DialogClose render={<Button variant="ghost" size="icon" />} aria-label="Close practice track completion">×</DialogClose></div>
        <DialogDescription>{result.completed.length} completed {result.completed.length === 1 ? "track" : "tracks"} saved in:</DialogDescription>
        <div className="dialog-body">
          <p className="practice-export-path select-text wrap-anywhere">{result.directory}</p>
          {result.error && <p role="alert" className="text-danger">{result.error}</p>}
          {(result.cancelled || result.error) && <ul className="practice-result-files">{result.completed.map((track) => <li key={track.key}>{track.name}<p className="muted wrap-anywhere">{track.path}</p></li>)}</ul>}
          {error && <p className="text-danger" role="alert">{error}</p>}
        </div>
        <div className="dialog-footer">
          <Button ref={doneButton} variant="outline" data-action="practice-export-done" onClick={onClose}>Done</Button>
          <Button data-action="practice-open-folder" disabled={opening} onClick={() => void openFolder()}>{opening ? "Opening…" : "Open folder"}</Button>
        </div>
      </> : <PracticeProgressContent progress={progress} total={total} cancelling={cancelling} onCancel={onCancel} />}
    </DialogContent>
  </Dialog>;
}
