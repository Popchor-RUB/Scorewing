import { useEffect, useState } from "react";
import { FolderClosedIcon } from "lucide-react";
import { Button } from "./components/ui/button.tsx";
import { invoke } from "@tauri-apps/api/core";

function useExistingScores(paths: string[]) {
  const [existing, setExisting] = useState<string[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    setExisting(null);
    setFailed(false);
    let active = true;
    void invoke<string[]>("existing_score_paths", { paths }).then(
      (result) => { if (active) setExisting(result); },
      () => { if (active) setFailed(true); },
    );
    return () => { active = false; };
  }, [paths, attempt]);
  return { existing, failed, retry: () => setAttempt((value) => value + 1) };
}

export function RecentScores({ paths, disabled, onOpen }: {
  paths: string[]; disabled: boolean; onOpen: (paths?: string[]) => void;
}) {
  const { existing, failed, retry } = useExistingScores(paths);
  return <section className="recent-scores" aria-labelledby="recent-scores-heading">
    <header><h1 id="recent-scores-heading">Last opened scores</h1></header>
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

export function RecentScoreButton({ disabled, reading, onOpen }: {
  disabled: boolean; reading: boolean; onOpen: () => void;
}) {
  return <Button disabled={disabled} data-action="open" onClick={onOpen}>
    <FolderClosedIcon data-icon="inline-start" aria-hidden="true" />
    {reading ? "Reading your score…" : "Open score"}
  </Button>;
}
