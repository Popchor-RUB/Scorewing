import { useEffect, useRef, useState } from "react";
import { Button } from "./components/ui/button.tsx";
import { Dialog, DialogContent, DialogTitle } from "./components/ui/dialog.tsx";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";

const SKIPPED_VERSION_KEY = "scorewing.skipped-update-version";

type Status = "idle" | "checking" | "available" | "downloading" | "ready" | "installing";

export function UpdateControl({ checkRequest, canInstall, onInstall, onPromptChange }: {
  checkRequest: number;
  canInstall: boolean;
  onInstall: (update: Update) => Promise<void>;
  onPromptChange: (open: boolean) => void;
}) {
  const [status, setStatus] = useState<Status>("idle");
  const [open, setOpen] = useState(false);
  const [supported, setSupported] = useState(true);
  const [error, setError] = useState("");
  const [progress, setProgress] = useState<number | undefined>();
  const update = useRef<Update | null>(null);
  const active = useRef(true);
  const working = useRef(false);
  const manualCheck = useRef(false);
  const prompting = useRef(false);

  async function closeUpdate() {
    const resource = update.current;
    update.current = null;
    try { await resource?.close(); } catch (error) { console.error(error); }
  }

  async function checkForUpdates(manual: boolean) {
    manualCheck.current ||= manual;
    if (manual) setOpen(true);
    if (working.current || update.current) return;
    working.current = true;
    setStatus("checking");
    setError("");
    try {
      const available = await invoke<boolean>("updater_supported");
      if (!active.current) return;
      setSupported(available);
      if (!available) { setStatus("idle"); return; }
      const found = await check({ timeout: 15_000 });
      update.current = found;
      if (active.current) {
        setStatus(found ? "available" : "idle");
        let skipped = false;
        try { skipped = localStorage.getItem(SKIPPED_VERSION_KEY) === found?.version; }
        catch { /* Still offer updates when storage is unavailable. */ }
        if (found && (manualCheck.current || !skipped)) setOpen(true);
      }
    } catch (error) {
      if (active.current) {
        setStatus("idle");
        if (manualCheck.current) setError(`Could not check for updates: ${String(error)}`);
      }
    } finally {
      working.current = false;
      if (!active.current) await closeUpdate();
    }
  }

  useEffect(() => {
    active.current = true;
    if (isTauri()) void checkForUpdates(false);
    return () => {
      active.current = false;
      if (!working.current) void closeUpdate();
    };
  }, []);

  useEffect(() => {
    if (checkRequest > 0 && active.current) void checkForUpdates(true);
  }, [checkRequest]);

  const found = update.current;
  const label = status === "ready" ? "Update ready" : status === "downloading" ? "Downloading update…"
    : status === "installing" ? "Installing update…" : found ? "Update available" : "Check for updates…";
  useEffect(() => {
    if (!isTauri()) return;
    void invoke("set_update_menu_state", { text: label, enabled: status !== "installing" })
      .catch((error) => { if (active.current) setError(`Could not update the update menu: ${String(error)}`); });
  }, [label, status]);

  async function download() {
    if (working.current || !canInstall || !update.current) return;
    working.current = true;
    setStatus("downloading");
    setError("");
    setProgress(undefined);
    let downloaded = 0;
    let total: number | undefined;
    let ready = false;
    try {
      await update.current.download((event) => {
        if (!active.current) return;
        if (event.event === "Started") total = event.data.contentLength;
        if (event.event === "Progress") downloaded += event.data.chunkLength;
        setProgress(total ? Math.min(100, downloaded / total * 100) : undefined);
      }, { timeout: 10 * 60_000 });
      if (active.current) { setStatus("ready"); ready = true; }
    } catch (error) {
      if (active.current) { setStatus("available"); setError(`Could not download the update: ${String(error)}`); }
    } finally {
      working.current = false;
      if (!active.current) await closeUpdate();
    }
    if (ready) await install();
  }

  async function install() {
    if (working.current || !canInstall || !update.current) return;
    working.current = true;
    setStatus("installing");
    setError("");
    try { await onInstall(update.current); }
    catch (error) {
      if (active.current) { setStatus("ready"); setError(`Could not install or restart: ${String(error)}`); }
    } finally {
      working.current = false;
      if (!active.current) await closeUpdate();
    }
  }

  async function openDownloads() {
    try { await invoke("open_update_downloads"); }
    catch (error) { if (active.current) setError(`Could not open GitHub Releases: ${String(error)}`); }
  }

  function skipVersion() {
    if (!update.current || working.current) return;
    try {
      localStorage.setItem(SKIPPED_VERSION_KEY, update.current.version);
      setOpen(false);
    } catch (error) {
      setError(`Could not remember the skipped version: ${String(error)}`);
    }
  }

  useEffect(() => {
    if (!open || status !== "available" || error || prompting.current) return;
    const found = update.current;
    if (!found) return;
    prompting.current = true;
    onPromptChange(true);
    void (async () => {
      let choice: "cancel" | "skip" | "install" | undefined;
      try {
        choice = await invoke("show_update_prompt", { currentVersion: found.currentVersion, version: found.version, canInstall });
      } catch (error) {
        if (active.current) setError(`Could not open the update dialog: ${String(error)}`);
      } finally {
        prompting.current = false;
        onPromptChange(false);
      }
      if (!active.current) return;
      if (choice === "cancel") setOpen(false);
      if (choice === "skip") skipVersion();
      if (choice === "install") await download();
    })();
  }, [open, status, error]);

  if (!isTauri()) return null;
  return <Dialog open={open && (status !== "available" || !!error)}
      onOpenChange={(value) => { if (status !== "installing") setOpen(value); }}>
      <DialogContent>
        <header className="dialog-header"><DialogTitle>Scorewing updates</DialogTitle></header>
        <div className="dialog-body space-y-3">
          <p role="status">{status === "checking" ? "Checking GitHub for updates…"
            : status === "downloading" ? `Downloading Scorewing ${found?.version}…`
            : status === "installing" ? "Installing the update and restarting Scorewing…"
            : status === "ready" ? `Scorewing ${found?.version} is ready to install.`
            : found ? `Scorewing ${found.version} is available. You have ${found.currentVersion}.`
            : !supported ? "In-app updates on Linux require the AppImage version. Download DEB/RPM updates from GitHub Releases."
            : !error ? "Scorewing is up to date." : "Update check failed."}</p>
          {found?.body && <p className="max-h-48 overflow-auto whitespace-pre-wrap text-sm">{found.body}</p>}
          {status === "downloading" && <progress className="update-download-progress" role="progressbar" aria-label="Update download" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress} max={100} value={progress} />}
          {(status === "available" || status === "ready") && <p className="muted">{canInstall ? "Installation will restart Scorewing."
            : "Close any open score or score selection and wait for current work to finish before installing. Closing a score discards its editing choices."}</p>}
          {error && <p role="alert" className="text-danger wrap-anywhere">{error}</p>}
        </div>
        <footer className="dialog-footer">
          <Button variant="outline" data-action="update-later" disabled={status === "installing"} onClick={() => setOpen(false)}>{status === "downloading" ? "Hide" : "Cancel"}</Button>
          {status === "available" && <>
            <Button variant="outline" data-action="skip-update" onClick={skipVersion}>Skip this version</Button>
            <Button data-action="download-update" disabled={!canInstall} onClick={() => void download()}>Download &amp; Install</Button>
          </>}
          {status === "ready" && <Button data-action="install-update" disabled={!canInstall} onClick={() => void install()}>Install and restart</Button>}
          {!supported && <Button data-action="update-downloads" onClick={() => void openDownloads()}>Open GitHub Releases</Button>}
          {status === "idle" && supported && error && <Button data-action="retry-update-check" onClick={() => void checkForUpdates(true)}>Try again</Button>}
        </footer>
      </DialogContent>
    </Dialog>;
}
