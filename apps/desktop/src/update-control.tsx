import { useEffect, useRef, useState } from "react";
import { Button, Description, Modal, ProgressBar } from "@heroui/react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";

type Status = "idle" | "checking" | "available" | "downloading" | "ready" | "installing";

export function UpdateControl({ checkRequest, canInstall, onInstall }: {
  checkRequest: number;
  canInstall: boolean;
  onInstall: (update: Update) => Promise<void>;
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
      if (active.current) setStatus(found ? "available" : "idle");
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
    if (working.current || !update.current) return;
    working.current = true;
    setStatus("downloading");
    setError("");
    setProgress(undefined);
    let downloaded = 0;
    let total: number | undefined;
    try {
      await update.current.download((event) => {
        if (!active.current) return;
        if (event.event === "Started") total = event.data.contentLength;
        if (event.event === "Progress") downloaded += event.data.chunkLength;
        setProgress(total ? Math.min(100, downloaded / total * 100) : undefined);
      }, { timeout: 10 * 60_000 });
      if (active.current) setStatus("ready");
    } catch (error) {
      if (active.current) { setStatus("available"); setError(`Could not download the update: ${String(error)}`); }
    } finally {
      working.current = false;
      if (!active.current) await closeUpdate();
    }
  }

  async function install() {
    if (working.current || !canInstall || status !== "ready" || !update.current) return;
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

  if (!isTauri()) return null;
  return <Modal.Backdrop isOpen={open} isDismissable={status !== "installing"} isKeyboardDismissDisabled={status === "installing"}
      onOpenChange={(value) => { if (status !== "installing") setOpen(value); }}>
      <Modal.Container size="md" scroll="inside"><Modal.Dialog>
        <Modal.Header><Modal.Heading>Scorewing updates</Modal.Heading></Modal.Header>
        <Modal.Body className="space-y-3">
          <p role="status">{status === "checking" ? "Checking GitHub for updates…"
            : status === "downloading" ? `Downloading Scorewing ${found?.version}…`
            : status === "installing" ? "Installing the update and restarting Scorewing…"
            : status === "ready" ? `Scorewing ${found?.version} is ready to install.`
            : found ? `Scorewing ${found.version} is available. You have ${found.currentVersion}.`
            : !supported ? "In-app updates on Linux require the AppImage version. Download DEB/RPM updates from GitHub Releases."
            : !error ? "Scorewing is up to date." : "Update check failed."}</p>
          {found?.body && <p className="max-h-48 overflow-auto whitespace-pre-wrap text-sm">{found.body}</p>}
          {status === "downloading" && <ProgressBar aria-label="Update download" value={progress ?? 0} isIndeterminate={progress === undefined}>
            <ProgressBar.Track><ProgressBar.Fill /></ProgressBar.Track>
          </ProgressBar>}
          {status === "ready" && <Description>{canInstall ? "Installation will restart Scorewing."
            : "Close any open score or score selection and wait for current work to finish before installing. Closing a score discards its editing choices."}</Description>}
          {error && <p role="alert" className="text-danger wrap-anywhere">{error}</p>}
        </Modal.Body>
        <Modal.Footer>
          <Button variant="secondary" data-action="update-later" isDisabled={status === "installing"} onPress={() => setOpen(false)}>Later</Button>
          {status === "available" && <Button data-action="download-update" onPress={() => void download()}>Download update</Button>}
          {status === "ready" && <Button data-action="install-update" isDisabled={!canInstall} onPress={() => void install()}>Install and restart</Button>}
          {!supported && <Button data-action="update-downloads" onPress={() => void openDownloads()}>Open GitHub Releases</Button>}
          {status === "idle" && supported && error && <Button data-action="retry-update-check" onPress={() => void checkForUpdates(true)}>Try again</Button>}
        </Modal.Footer>
      </Modal.Dialog></Modal.Container>
    </Modal.Backdrop>;
}
