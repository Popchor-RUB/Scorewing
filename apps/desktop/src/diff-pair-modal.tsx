import { useEffect, useRef } from "react";
import { Button } from "./components/ui/button.tsx";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./components/ui/dialog.tsx";
import type { Inspection } from "./model.ts";

export function DiffFileSelectors({ older, newer, disabled, onChoose, onSwap }: {
  older?: Inspection; newer: Inspection; disabled: boolean;
  onChoose: (version: "old" | "new") => void; onSwap: () => void;
}) {
  function file(version: "old" | "new", inspection?: Inspection) {
    const label = version === "old" ? "Older version" : "Newer version";
    const action = version === "old" ? "older" : "newer";
    const command = `${inspection ? "Replace" : "Choose"} ${action} version`;
    return <section data-version={version} className="diff-file" aria-label={label}>
      <h3 className="diff-file-label">{label}</h3>
      <div className="diff-file-card">
        <div className="diff-file-summary">
          <svg className="diff-file-icon" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="M14 2H5v20h14V7z" /><path d="M14 2v5h5M8 12h8M8 16h8" /></svg>
          <div><p className="diff-file-name">{inspection?.name ?? "No file selected"}</p>
            <p className="diff-file-path">{inspection?.path ?? "Choose the earlier score to compare."}</p></div>
        </div>
        <Button variant="outline" data-action={`open-${action}`} aria-label={command} disabled={disabled} onClick={() => onChoose(version)}>{command}</Button>
      </div>
    </section>;
  }
  return <div className="diff-files">
    {file("old", older)}
    <Button size="icon" variant="outline" className="diff-swap" aria-label="Swap old and new files" data-action="swap-diff"
      disabled={disabled || !older} onClick={onSwap}><span aria-hidden="true">⇄</span></Button>
    {file("new", newer)}
  </div>;
}

export function DiffPairModal({ pair, busy, closing, error, status, onChoose, onSwap, onCancel, onCompare }: {
  pair: [Inspection, Inspection]; busy: boolean; closing: boolean; error: string;
  status: string; onChoose: (version: "old" | "new") => void;
  onSwap: () => void; onCancel: () => void; onCompare: () => void;
}) {
  const previous = useRef(document.activeElement === document.body
    ? document.querySelector<HTMLElement>('[data-action="open"]') : document.activeElement);
  useEffect(() => () => {
    requestAnimationFrame(() => {
      if (previous.current instanceof HTMLElement && previous.current.isConnected) previous.current.focus();
    });
  }, []);
  return <Dialog open onOpenChange={(open) => { if (!open && !busy) onCancel(); }}>
    <DialogContent className="diff-pair-dialog" finalFocus={false}>
      <header className="dialog-header"><DialogTitle>Compare score versions</DialogTitle><DialogDescription>Confirm which score is old and which is new. Changes will be marked in the exported newer version.</DialogDescription></header>
      <div className="dialog-body">
        <DiffFileSelectors older={pair[0]} newer={pair[1]} disabled={busy || closing} onChoose={onChoose} onSwap={onSwap} />
        {error && <p role="alert" className="mt-4 text-danger">{error}</p>}
        {!error && status && <p role="status" className="mt-4 whitespace-pre-line">{status}</p>}
      </div>
      <footer className="dialog-footer"><Button variant="outline" data-action="cancel-diff" disabled={busy} onClick={onCancel}>Cancel</Button><Button data-action="compare-diff" disabled={busy || closing} onClick={onCompare}>{busy ? "Exporting…" : "Export diff"}</Button></footer>
    </DialogContent>
  </Dialog>;
}
