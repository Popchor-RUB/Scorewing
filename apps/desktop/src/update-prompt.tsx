import { useState } from "react";
import { createRoot } from "react-dom/client";
import { Button, Description, Link, Surface } from "@heroui/react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import appIcon from "../src-tauri/icons/128x128.png";

declare global {
  interface Window {
    __SCOREWING_UPDATE_PROMPT__: { currentVersion: string; version: string; canInstall: boolean };
  }
}

function UpdatePrompt() {
  const { currentVersion, version, canInstall } = window.__SCOREWING_UPDATE_PROMPT__;
  const [error, setError] = useState("");
  const [answering, setAnswering] = useState(false);

  async function answer(choice: "cancel" | "skip" | "install") {
    if (answering) return;
    setAnswering(true);
    try { await emitTo("update-prompt", "update-prompt-choice", choice); }
    catch (error) { setAnswering(false); setError(`Could not close the update dialog: ${String(error)}`); }
  }

  return <Surface className="update-prompt" role="dialog" aria-modal="true" aria-labelledby="update-prompt-heading"
    onKeyDown={(event) => { if (event.key === "Escape") void answer("cancel"); }}>
    <div className="update-prompt-content">
      <img src={appIcon} width="80" height="80" alt="Scorewing" />
      <div className="space-y-3">
        <h1 id="update-prompt-heading" className="text-xl font-semibold">A new version is available</h1>
        <p>Scorewing <strong>{version}</strong> is available.<br />You currently have <strong>{currentVersion}</strong> installed.</p>
        <div><Link href="https://github.com/Popchor-RUB/Scorewing/releases/latest" data-action="update-changelog" className="text-accent underline underline-offset-4"
          onClick={(event) => {
            event.preventDefault();
            void invoke("open_update_downloads").catch((error) => setError(`Could not open GitHub Releases: ${String(error)}`));
          }}>View changelog on GitHub</Link></div>
        <Description className="block">{canInstall ? "Installation will restart Scorewing."
          : "Close any open score or score selection and wait for current work to finish before installing. Closing a score discards its editing choices."}</Description>
        {error && <p role="alert" className="text-danger wrap-anywhere">{error}</p>}
      </div>
    </div>
    <footer className="update-prompt-actions">
      <Button variant="secondary" data-action="update-later" autoFocus isDisabled={answering} onPress={() => void answer("cancel")}>Cancel</Button>
      <Button variant="secondary" data-action="skip-update" isDisabled={answering} onPress={() => void answer("skip")}>Skip this version</Button>
      <Button data-action="download-update" isDisabled={answering || !canInstall} onPress={() => void answer("install")}>Download &amp; Install</Button>
    </footer>
  </Surface>;
}

export function mountApp(container: HTMLElement): () => void {
  const root = createRoot(container);
  root.render(<UpdatePrompt />);
  return () => root.unmount();
}
