import { useEffect, useState } from "react";
import { Button, Description, Modal, ProgressBar } from "@heroui/react";
import { invoke } from "@tauri-apps/api/core";
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
    <Modal.Header><Modal.Heading>Exporting practice tracks</Modal.Heading></Modal.Header>
    <Modal.Body className="practice-progress space-y-3">
      <p role="status">{cancelling ? "Cancelling…" : progress?.stage ?? "Choose a folder for your practice tracks."} · {completed} / {total} tracks completed</p>
      <p>{running} {running === 1 ? "export" : "exports"} currently running</p>
      <ProgressBar aria-label="Practice track rendering" value={value}>
        <ProgressBar.Track><ProgressBar.Fill className="duration-100 ease-linear" /></ProgressBar.Track>
      </ProgressBar>
    </Modal.Body>
    <Modal.Footer>
      <Button variant="secondary" isDisabled={cancelling} data-action="practice-cancel" onPress={onCancel}>
        {cancelling ? "Cancelling…" : "Cancel rendering"}
      </Button>
    </Modal.Footer>
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

  async function openFolder() {
    if (opening || !result) return;
    setOpening(true);
    setError("");
    try { await invoke("open_practice_export_folder", { directory: result.directory }); }
    catch (error) { setError(`Could not open the folder: ${String(error)}`); }
    finally { setOpening(false); }
  }

  return <Modal.Backdrop isOpen isDismissable={!!result} isKeyboardDismissDisabled={!result}
    onOpenChange={(open) => { if (!open && result) onClose(); }}>
    <Modal.Container size="md" scroll="inside">
      <Modal.Dialog>
        {result ? <>
        <Modal.CloseTrigger aria-label="Close practice track completion" />
        <Modal.Header>
          <svg className="practice-export-hero w-full text-accent" aria-hidden="true" focusable="false" viewBox="0 0 360 160">
            <ellipse cx="180" cy="137" rx="118" ry="9" fill="currentColor" opacity=".08" />
            <g fill="none" stroke="currentColor" opacity=".16" strokeWidth="2">
              <path d="M35 62h290M35 76h290M35 90h290M35 104h290M35 118h290" />
            </g>
            <g className="dancing-note" fill="currentColor">
              <ellipse cx="92" cy="103" rx="13" ry="9" transform="rotate(-20 92 103)" />
              <path d="M101 101V43h5c0 17 24 14 17 38-2-13-13-13-17-18v38Z" />
            </g>
            <g className="dancing-note" fill="currentColor">
              <ellipse cx="170" cy="106" rx="13" ry="9" transform="rotate(-20 170 106)" />
              <ellipse cx="215" cy="95" rx="13" ry="9" transform="rotate(-20 215 95)" />
              <path d="M179 105V40l49-12v66h-5V46l-39 10v49Z" />
            </g>
            <g className="dancing-note" fill="currentColor">
              <ellipse cx="275" cy="105" rx="12" ry="9" transform="rotate(-20 275 105)" />
              <path d="M283 104V46h5v58Z" />
            </g>
            <g fill="currentColor" opacity=".4">
              <path d="m56 35 2 6 6 2-6 2-2 6-2-6-6-2 6-2Zm246-13 2 6 6 2-6 2-2 6-2-6-6-2 6-2Z" />
              <circle cx="143" cy="26" r="3" />
            </g>
          </svg>
          <Modal.Heading>Practice tracks created</Modal.Heading>
          <Description>{result.completed.length} completed {result.completed.length === 1 ? "track" : "tracks"} saved in:</Description>
        </Modal.Header>
        <Modal.Body>
          <p className="practice-export-path select-text wrap-anywhere rounded-xl bg-default p-3 text-sm">{result.directory}</p>
          {error && <p className="mt-3 text-danger" role="alert">{error}</p>}
        </Modal.Body>
        <Modal.Footer>
          <Button variant="secondary" data-action="practice-export-done" onPress={onClose}>Done</Button>
          <Button variant="primary" data-action="practice-open-folder" isDisabled={opening} onPress={() => void openFolder()}>
            {opening ? "Opening…" : "Open folder"}
          </Button>
        </Modal.Footer>
        </> : <PracticeProgressContent progress={progress} total={total} cancelling={cancelling} onCancel={onCancel} />}
      </Modal.Dialog>
    </Modal.Container>
  </Modal.Backdrop>;
}
