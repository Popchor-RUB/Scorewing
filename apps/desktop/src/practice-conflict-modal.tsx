import { useRef, useState } from "react";
import { Button } from "./components/ui/button.tsx";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./components/ui/dialog.tsx";
import { RadioGroup, RadioGroupItem } from "./components/ui/radio-group.tsx";
import type { PracticeConflict, PracticeOutputChoice } from "./model.ts";

export function PracticeConflictModal({ conflict, onResolve }: {
  conflict: PracticeConflict;
  onResolve: (choice: PracticeOutputChoice) => Promise<void>;
}) {
  const [choice, setChoice] = useState<"overwrite" | "clear">("overwrite");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  const cancelButton = useRef<HTMLButtonElement>(null);

  async function resolve(answer: PracticeOutputChoice) {
    if (submitting) return;
    setSubmitting(true); setError("");
    try { await onResolve(answer); }
    catch (error) { setError(String(error)); setSubmitting(false); }
  }

  return <Dialog open disablePointerDismissal onOpenChange={(open, details) => {
    if (!open) { details.cancel(); void resolve("cancel"); }
  }}>
    <DialogContent className="practice-conflict-dialog" initialFocus={cancelButton} finalFocus={() => document.querySelector<HTMLButtonElement>('[data-action="practice-export"]')}>
      <DialogTitle>Files already exist</DialogTitle>
      <DialogDescription>{conflict.conflicts} of the {conflict.total} tracks in this export already exist. Choose how to continue.</DialogDescription>
      <div className="dialog-body">
        <p className="practice-export-path select-text wrap-anywhere">{conflict.directory}</p>
        <RadioGroup value={choice} onValueChange={(value) => setChoice(value as "overwrite" | "clear")} disabled={submitting} aria-label="Existing practice tracks" className="practice-output-choices">
          <label className="practice-output-choice" data-selected={choice === "overwrite"}>
            <RadioGroupItem value="overwrite" data-action="practice-overwrite-choice" />
            <span><strong>Overwrite matching files</strong><span className="muted">Replace the {conflict.conflicts} matching tracks.<br />Keep all other files and folders.</span></span>
          </label>
          <label className="practice-output-choice practice-output-clear" data-selected={choice === "clear"}>
            <RadioGroupItem value="clear" disabled={!!conflict.clear_error} aria-describedby={conflict.clear_error ? "practice-clear-error" : undefined} data-action="practice-clear-choice" />
            <span><strong>Clear folder before exporting</strong><span>Delete everything inside this folder,<br />including unrelated files and subfolders.</span></span>
          </label>
        </RadioGroup>
        {conflict.clear_error && <p id="practice-clear-error" className="text-danger">{conflict.clear_error}</p>}
        <p className="text-danger">Replacing or deleting existing files cannot be undone.</p>
        {error && <p role="alert" className="text-danger">{error}</p>}
      </div>
      <div className="dialog-footer">
        <Button ref={cancelButton} variant="outline" disabled={submitting} data-action="practice-conflict-cancel" onClick={() => void resolve("cancel")}>Cancel</Button>
        <Button variant={choice === "clear" ? "destructive" : "default"} disabled={submitting} data-action="practice-conflict-confirm" onClick={() => void resolve(choice)}>{choice === "clear" ? "Clear folder & export" : "Overwrite & export"}</Button>
      </div>
    </DialogContent>
  </Dialog>;
}
