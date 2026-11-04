import { useEffect, useState } from "react";
import { Menu } from "@base-ui/react/menu";
import { Button } from "./components/ui/button.tsx";
import { invoke } from "@tauri-apps/api/core";

function useExistingScores(paths: string[], enabled: boolean) {
  const [existing, setExisting] = useState<string[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    setExisting(null);
    setFailed(false);
    if (!enabled) return;
    let active = true;
    void invoke<string[]>("existing_score_paths", { paths }).then(
      (result) => { if (active) setExisting(result); },
      () => { if (active) setFailed(true); },
    );
    return () => { active = false; };
  }, [enabled, paths, attempt]);
  return { existing, failed, retry: () => setAttempt((value) => value + 1) };
}

export function RecentScores({ paths, disabled, onOpen }: {
  paths: string[]; disabled: boolean; onOpen: (paths?: string[]) => void;
}) {
  const { existing, failed, retry } = useExistingScores(paths, true);
  return <section className="recent-scores" aria-labelledby="recent-scores-heading">
    <header><h1 id="recent-scores-heading">Recent scores</h1><p className="muted">Open a recent score or choose a MusicXML file.</p></header>
    {failed ? <div className="recent-scores-status"><p role="status">Recent scores unavailable</p><Button variant="outline" disabled={disabled} onClick={retry}>Try again</Button></div>
      : existing === null ? <p role="status" className="recent-scores-status muted">Checking recent scores…</p>
      : existing.length === 0 ? <p className="recent-scores-status muted">No recent scores. Open a score to get started.</p>
      : <ul className="recent-scores-list">{existing.map((path) => <li key={path}>
        <Button variant="ghost" className="recent-score-row" disabled={disabled} onClick={() => onOpen([path])}>
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="M14 2H5v20h14V7z" /><path d="M14 2v5h5M8 12h8M8 16h8" /></svg>
          <span className="recent-score-details"><span className="recent-menu-name">{path.split(/[\\/]/).pop()}</span><span className="recent-menu-path">{path.replace(/[\\/][^\\/]+$/, "")}</span></span>
          <span className="recent-score-arrow" aria-hidden="true">→</span>
        </Button>
      </li>)}</ul>}
  </section>;
}

export function RecentScoreButton({ paths, disabled, reading, onOpen }: {
  paths: string[]; disabled: boolean; reading: boolean; onOpen: (paths?: string[]) => void;
}) {
  const [isOpen, setOpen] = useState(false);
  const { existing, failed } = useExistingScores(paths, isOpen && !disabled);
  useEffect(() => { if (disabled) setOpen(false); }, [disabled]);

  function changeOpen(open: boolean) {
    setOpen(open);
  }

  const status = failed ? "Recent scores unavailable" : existing === null ? "Checking recent scores…" : "No recent scores";
  return <div role="group" aria-label="Open a score" className="recent-score-buttons">
    <Button disabled={disabled} data-action="open" onClick={() => onOpen()}>{reading ? "Reading your score…" : "Open score"}</Button>
    <Menu.Root open={isOpen && !disabled} onOpenChange={changeOpen}>
      <Menu.Trigger disabled={disabled} render={<Button size="icon" variant="outline" />} aria-label="Recently opened scores"><span aria-hidden="true">⌄</span></Menu.Trigger>
      <Menu.Portal><Menu.Positioner sideOffset={4} align="end" className="recent-menu-positioner">
        <Menu.Popup aria-label="Recently opened scores" className="recent-menu">
          {existing?.length ? existing.map((path) => <Menu.Item key={path} label={path} className="recent-menu-item" onClick={() => { changeOpen(false); onOpen([path]); }}>
            <span className="recent-menu-name">{path.split(/[\\/]/).pop()}</span><span className="recent-menu-path">{path}</span>
          </Menu.Item>) : <Menu.Item disabled className="recent-menu-item">{status}</Menu.Item>}
        </Menu.Popup>
      </Menu.Positioner></Menu.Portal>
    </Menu.Root>
  </div>;
}
