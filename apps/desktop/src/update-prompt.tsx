import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { Button } from "./components/ui/button.tsx";
import { ButtonGroup } from "./components/ui/button-group.tsx";
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from "./components/ui/dropdown-menu.tsx";
import { Progress } from "./components/ui/progress.tsx";
import { ChevronDown } from "lucide-react";
import { Field, FieldGroup, FieldLabel } from "./components/ui/field.tsx";
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "./components/ui/select.tsx";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import appIcon from "../src-tauri/icons/128x128.png";

export type ReleaseChannel = "stable" | "nightly";
export type UpdateStatus = "idle" | "checking" | "available" | "confirm-downgrade" | "downloading" | "ready" | "installing" | "scheduled";
export type UpdateWindowState = {
  revision: number;
  channel: ReleaseChannel;
  currentVersion: string;
  status: UpdateStatus;
  supported: boolean;
  canInstall: boolean;
  version?: string;
  downgrade: boolean;
  error: string;
  progress?: number;
  deferred?: boolean;
};
export type UpdateAction = { revision?: number } & (
  { type: "set-channel"; channel: ReleaseChannel } |
  { type: "ready" | "cancel" | "cancel-download" | "skip" | "check" | "download" | "download-on-next-start" | "install" | "review-downgrade" | "confirm-downgrade" | "back" | "open-downloads" }
);

declare global {
  interface Window { __SCOREWING_UPDATE_STATE__: UpdateWindowState }
}

export function UpdatePanel({ state, onAction, dialogTitle }: {
  state: UpdateWindowState;
  onAction: (action: UpdateAction) => void;
  dialogTitle?: boolean;
}) {
  const { status, channel, version, currentVersion, error, canInstall, supported, downgrade } = state;
  const locked = status === "downloading" || status === "installing" || status === "scheduled";
  const send = (action: UpdateAction) => onAction({ ...action, revision: state.revision });
  const releasePage = `https://github.com/Popchor-RUB/Scorewing/releases/${channel === "nightly" ? "tag/nightly" : "latest"}`;
  return <section className="update-prompt" role={dialogTitle ? undefined : "dialog"} aria-labelledby="update-prompt-heading"
    onKeyDown={(event) => { if (event.key === "Escape" && !event.defaultPrevented) send({ type: "cancel" }); }}>
    <div className="update-prompt-content">
      <img src={appIcon} width="80" height="80" alt="Scorewing" />
      <div className="update-prompt-details">
        <h1 id="update-prompt-heading" className="text-xl font-semibold">Scorewing updates</h1>
        <FieldGroup><Field orientation="horizontal" data-disabled={locked} className="update-channel-field">
          <FieldLabel htmlFor="update-channel">Release channel</FieldLabel>
          <Select value={channel} disabled={locked} onValueChange={(value) => {
            if (value === "stable" || value === "nightly") send({ type: "set-channel", channel: value });
          }}>
            <SelectTrigger id="update-channel" data-action="update-channel" className="w-44"><SelectValue>{channel === "stable" ? "Stable" : "Nightly"}</SelectValue></SelectTrigger>
            <SelectContent><SelectGroup><SelectItem value="stable">Stable</SelectItem><SelectItem value="nightly">Nightly</SelectItem></SelectGroup></SelectContent>
          </Select>
        </Field></FieldGroup>
        {status === "downloading" && <a href={releasePage} data-action="update-changelog" className="text-accent underline underline-offset-4"
          onClick={(event) => { event.preventDefault(); send({ type: "open-downloads" }); }}>View {channel} release notes on GitHub</a>}
        <p role="status" className="update-prompt-status">{status === "checking" ? `Checking ${channel} releases…`
          : status === "downloading" ? `Downloading Scorewing ${version}…`
          : status === "scheduled" ? `Scorewing ${version} will install on next start.`
          : status === "installing" ? "Installing the update and restarting Scorewing…"
          : status === "ready" ? `Scorewing ${version} is ready to install.`
          : status === "confirm-downgrade" ? `Downgrade to Scorewing ${version}?`
          : version ? downgrade ? `The ${channel} channel has an older version: ${version}.` : `Scorewing ${version} is available.`
          : !supported ? "In-app updates require the Linux AppImage."
          : error ? "Update check failed." : `Scorewing is up to date on ${channel}.`}
          {currentVersion && status !== "downloading" && <><br />You currently have {currentVersion} installed.</>}
        </p>
        {status !== "downloading" && <a href={releasePage} data-action="update-changelog" className="text-accent underline underline-offset-4"
          onClick={(event) => { event.preventDefault(); send({ type: "open-downloads" }); }}>View {channel} release notes on GitHub</a>}
        {status === "downloading" && <Progress aria-label="Update download" value={state.progress ?? null} />}
        {error ? <p role="alert" className="text-danger wrap-anywhere">{error}</p>
          : !supported ? <p className="muted">Download updates from GitHub Releases. DEB/RPM installs cannot update in-app.</p>
          : status === "confirm-downgrade" ? <p className="muted">This installs an older version and restarts Scorewing. Your saved scores will remain on disk.</p>
          : status === "checking" ? <p className="muted">Changing the release channel starts a new check.</p>
          : status === "idle" ? <p className="muted">{channel === "stable" ? "Choose Nightly to check development builds." : "Choose Stable to check published releases."}</p>
          : status === "downloading" ? <p className="muted">{state.deferred ? <>Downloading in the background.<br />Scorewing will install this update on next start.</> : "Installation will restart Scorewing when it is safe."}</p>
          : status === "scheduled" ? <p className="muted">You can keep working. Close and reopen Scorewing<br />when you are ready to update.</p>
          : status === "installing" ? <p className="muted">Please wait until Scorewing restarts.</p>
          : (status === "available" || status === "ready") && <p className="muted">{canInstall ? downgrade ? "Review and confirm before installing an older version." : "Installation will restart Scorewing."
            : status === "available" && !downgrade ? <>Close open scores and finish current work to install now.<br />Or choose Install on next start from the arrow menu.</>
            : "Close any open score or score selection and wait for current work to finish before installing. Closing a score discards its editing choices."}</p>}
      </div>
    </div>
    <footer className="update-prompt-actions">
      {status === "confirm-downgrade" ? <>
        <Button variant="outline" data-action="back-update" onClick={() => send({ type: "back" })}>Back</Button>
        <Button data-action="confirm-downgrade" disabled={!canInstall} onClick={() => send({ type: "confirm-downgrade" })}>Downgrade &amp; restart</Button>
      </> : <>
        {status === "downloading" && <Button variant="destructive" data-action="cancel-download" onClick={() => send({ type: "cancel-download" })}>Cancel</Button>}
        <Button variant="outline" data-action="update-later" autoFocus disabled={status === "installing"} onClick={() => send({ type: "cancel" })}>{status === "downloading" ? "Hide" : status === "scheduled" ? "Close" : "Cancel"}</Button>
        {status === "available" && <>
          {error ? <Button variant="outline" data-action="retry-update-check" onClick={() => send({ type: "check" })}>Check again</Button>
            : !downgrade && <Button variant="outline" data-action="skip-update" onClick={() => send({ type: "skip" })}>Skip this version</Button>}
          {downgrade || !supported ? <Button data-action={downgrade ? "review-downgrade" : "download-update"} disabled={!canInstall} onClick={() => send({ type: downgrade ? "review-downgrade" : "download" })}>{downgrade ? "Review downgrade" : "Download & Install"}</Button> :
            <ButtonGroup aria-label="Update installation options">
              <Button data-action="download-update" disabled={!canInstall} onClick={() => send({ type: "download" })}>Download &amp; Install</Button>
              <DropdownMenu>
                <DropdownMenuTrigger render={<Button aria-label="More update options" data-action="more-update-options" size="icon"><ChevronDown /></Button>} />
                <DropdownMenuContent side="top" align="end" className="w-50"><DropdownMenuGroup>
                  <DropdownMenuItem data-action="download-on-next-start" onClick={() => send({ type: "download-on-next-start" })}>Install on next start</DropdownMenuItem>
                </DropdownMenuGroup></DropdownMenuContent>
              </DropdownMenu>
            </ButtonGroup>}
        </>}
        {status === "ready" && <Button data-action="install-update" disabled={!canInstall} onClick={() => send({ type: "install" })}>Install and restart</Button>}
        {!supported && <Button data-action="update-downloads" onClick={() => send({ type: "open-downloads" })}>Open GitHub Releases</Button>}
        {status === "idle" && supported && <Button data-action="retry-update-check" onClick={() => send({ type: "check" })}>{error ? "Try again" : "Check again"}</Button>}
      </>}
    </footer>
  </section>;
}

function UpdatePrompt() {
  const [state, setState] = useState(window.__SCOREWING_UPDATE_STATE__);
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    const subscription = listen<UpdateWindowState>("update-prompt-state", (event) => {
      if (!disposed) { setState(event.payload); setError(""); }
    });
    void subscription.then(() => { if (!disposed) return invoke("update_prompt_action", { action: { type: "ready" } }); })
      .catch((error) => { if (!disposed) setError(`Could not connect to updates: ${String(error)}`); });
    return () => { disposed = true; void subscription.then((unlisten) => unlisten()).catch(console.error); };
  }, []);
  return <UpdatePanel state={{ ...state, error: error || state.error }} onAction={(action) => {
    void invoke("update_prompt_action", { action }).catch((error) => setError(`Could not send the update action: ${String(error)}`));
  }} />;
}

export function mountApp(container: HTMLElement): () => void {
  const root = createRoot(container);
  root.render(<UpdatePrompt />);
  return () => root.unmount();
}
