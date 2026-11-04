import { Button } from "./components/ui/button.tsx";
import { DiffFileSelectors } from "./diff-pair-modal.tsx";
import type { Inspection } from "./model.ts";

export function DiffWorkspace({ older, newer, busy, exporting, onChoose, onSwap, onExport }: {
  older?: Inspection; newer: Inspection; busy: boolean; exporting: boolean;
  onChoose: (version: "old" | "new") => void; onSwap: () => void; onExport: () => void;
}) {
  return <section className="diff-card diff-workspace" aria-labelledby="diff-heading">
    <div className="diff-content">
      <header><h2 id="diff-heading">Compare score versions</h2><p className="muted">Choose the older and newer score. Changes will be marked in the exported newer version.</p></header>
      <DiffFileSelectors older={older} newer={newer} disabled={busy} onChoose={onChoose} onSwap={onSwap} />
      <div className="diff-note"><p>Changed music is marked red. Both source files remain intact.</p><p>Includes changes to notes, lyrics, dynamics, chord symbols, ties, and slurs.</p></div>
    </div>
    <footer className="workspace-command-bar"><span className="muted">{older ? "Older and newer versions selected" : "Choose an older version to continue"}</span>
      <Button data-action="export" disabled={busy || !older} onClick={onExport}>{exporting ? "Exporting…" : "Export marked newer version"}</Button>
    </footer>
  </section>;
}
