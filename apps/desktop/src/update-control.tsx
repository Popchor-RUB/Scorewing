import { useEffect, useRef, useState } from "react";
import { Dialog, DialogContent, DialogTitle } from "./components/ui/dialog.tsx";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Update } from "@tauri-apps/plugin-updater";
import { UpdatePanel, type ReleaseChannel, type UpdateAction, type UpdateWindowState } from "./update-prompt.tsx";

const CHANNEL_KEY = "scorewing.update-channel";
const skipKey = (channel: ReleaseChannel) => `scorewing.skipped-update-version${channel === "nightly" ? ".nightly" : ""}`;
type Metadata = ConstructorParameters<typeof Update>[0] & { downgrade: boolean };
type Environment = { currentVersion: string; supported: boolean };

export function UpdateControl({ checkRequest, canInstall, onInstall }: {
  checkRequest: number;
  canInstall: boolean;
  onInstall: (update: Update) => Promise<void>;
}) {
  const [state, setState] = useState<UpdateWindowState>({ revision: 0, channel: "stable", currentVersion: "", status: "idle", supported: true, canInstall, downgrade: false, error: "" });
  const stateRef = useRef(state);
  const [fallback, setFallback] = useState(false);
  const [windowError, setWindowError] = useState("");
  const update = useRef<Update | null>(null);
  const active = useRef(true);
  const visible = useRef(false);
  const working = useRef(false);
  const request = useRef(0);
  const approvedDowngrade = useRef<number | undefined>(undefined);
  const environment = useRef<Promise<Environment> | undefined>(undefined);
  const selectedChannel = useRef<ReleaseChannel | undefined>(undefined);
  const windowSync = useRef(Promise.resolve());
  const canInstallRef = useRef(canInstall);
  const onInstallRef = useRef(onInstall);
  const view = { ...state, canInstall };
  canInstallRef.current = canInstall;
  onInstallRef.current = onInstall;

  function change(patch: Partial<UpdateWindowState>) {
    if (!active.current) return;
    stateRef.current = { ...stateRef.current, ...patch };
    setState(stateRef.current);
  }

  async function closeUpdate() {
    const resource = update.current;
    update.current = null;
    try { await resource?.close(); } catch (error) { console.error(error); }
  }

  async function syncWindow(show = false) {
    try {
      const synced = windowSync.current.then(() => invoke("sync_update_prompt", { state: { ...stateRef.current, canInstall: canInstallRef.current }, show }));
      windowSync.current = synced.then(() => undefined, () => undefined);
      await synced;
      if (show && active.current) { setFallback(false); setWindowError(""); }
      if (show && !visible.current) await invoke("hide_update_prompt");
    } catch (error) {
      if (active.current && visible.current) {
        setWindowError(`Could not open the update window: ${String(error)}`);
        setFallback(true);
      }
    }
  }

  function showWindow() {
    visible.current = true;
    void syncWindow(true);
  }

  async function checkForUpdates(manual: boolean, channel?: ReleaseChannel) {
    if (working.current || stateRef.current.status === "installing") {
      if (manual) showWindow();
      return;
    }
    const revision = ++request.current;
    approvedDowngrade.current = undefined;
    change({ revision, status: "checking", version: undefined, downgrade: false, progress: undefined, error: "", ...(channel && { channel }) });
    if (manual) showWindow();
    await closeUpdate();
    try {
      environment.current ??= invoke<Environment>("update_environment").catch((error) => { environment.current = undefined; throw error; });
      const env = await environment.current;
      if (!active.current || revision !== request.current) return;
      if (!selectedChannel.current) {
        try {
          const saved = localStorage.getItem(CHANNEL_KEY);
          if (saved === "stable" || saved === "nightly") selectedChannel.current = saved;
        } catch { /* The installed channel still works without storage. */ }
        selectedChannel.current ??= /-nightly(?:\.|$)/.test(env.currentVersion) ? "nightly" : "stable";
      }
      const chosen = channel ?? selectedChannel.current;
      change({ currentVersion: env.currentVersion, supported: env.supported, channel: chosen });
      if (!env.supported) { change({ status: "idle" }); return; }
      const metadata = await invoke<Metadata | null>("check_update", { channel: chosen, manual });
      const found = metadata ? new Update(metadata) : null;
      if (!active.current || revision !== request.current) { await found?.close(); return; }
      update.current = found;
      change({ status: found ? "available" : "idle", version: found?.version, downgrade: metadata?.downgrade ?? false });
      let skipped = false;
      try { skipped = localStorage.getItem(skipKey(chosen)) === found?.version; }
      catch { /* Still offer updates when storage is unavailable. */ }
      if (found && !metadata?.downgrade && !manual && !skipped && !visible.current) showWindow();
    } catch (error) {
      if (active.current && revision === request.current) {
        change({ status: "idle", error: manual || visible.current ? `Could not check for updates: ${String(error)}` : "" });
      }
    }
  }

  async function install() {
    const current = stateRef.current;
    if (working.current || !canInstallRef.current || !update.current || current.status !== "ready") return;
    if (current.downgrade && approvedDowngrade.current !== current.revision) return;
    working.current = true;
    change({ status: "installing", error: "" });
    try { await onInstallRef.current(update.current); }
    catch (error) { change({ status: "ready", error: `Could not install or restart: ${String(error)}` }); }
    finally {
      working.current = false;
      if (!active.current) await closeUpdate();
    }
  }

  async function download(downgradeConfirmed = false) {
    const current = stateRef.current;
    if (working.current || !canInstallRef.current || !update.current) return;
    if (current.downgrade) {
      if (!downgradeConfirmed || current.status !== "confirm-downgrade") return;
      approvedDowngrade.current = current.revision;
    } else if (current.status !== "available") return;
    working.current = true;
    change({ status: "downloading", error: "", progress: undefined });
    let downloaded = 0;
    let total: number | undefined;
    let ready = false;
    try {
      await update.current.download((event) => {
        if (!active.current) return;
        if (event.event === "Started") total = event.data.contentLength;
        if (event.event === "Progress") downloaded += event.data.chunkLength;
        change({ progress: total ? Math.min(100, downloaded / total * 100) : undefined });
      }, { timeout: 10 * 60_000 });
      if (active.current) { change({ status: "ready" }); ready = true; }
    } catch (error) {
      change({ status: "available", error: `Could not download the update: ${String(error)}. Check again if this nightly build is no longer available.` });
    } finally {
      working.current = false;
      if (!active.current) await closeUpdate();
    }
    if (ready) await install();
  }

  async function action(action: UpdateAction) {
    if (!active.current) return;
    const current = stateRef.current;
    if (action.type === "ready") { await syncWindow(); return; }
    if (action.revision != null && action.revision !== current.revision) return;
    if (action.type === "cancel") {
      if (current.status === "installing") return;
      visible.current = false;
      setFallback(false);
      try { await invoke("hide_update_prompt"); } catch (error) { console.error(error); }
      return;
    }
    if (action.type === "open-downloads") {
      try { await invoke("open_update_downloads", { channel: current.channel }); }
      catch (error) { if (current.revision === stateRef.current.revision) change({ error: `Could not open GitHub Releases: ${String(error)}` }); }
      return;
    }
    if (working.current || current.status === "installing") return;
    if (action.type === "set-channel") {
      if (action.channel === current.channel) return;
      selectedChannel.current = action.channel;
      let storageError = "";
      try { localStorage.setItem(CHANNEL_KEY, action.channel); }
      catch (error) { storageError = `Could not remember the release channel: ${String(error)}`; }
      const revision = request.current + 1;
      await checkForUpdates(true, action.channel);
      if (storageError && request.current === revision) change({ error: storageError });
    } else if (action.type === "check") await checkForUpdates(true);
    else if (action.type === "skip" && update.current && current.status === "available") {
      try {
        localStorage.setItem(skipKey(current.channel), update.current.version);
        await actionCancel();
      } catch (error) { change({ error: `Could not remember the skipped version: ${String(error)}` }); }
    } else if (action.type === "review-downgrade" && current.downgrade && current.status === "available") change({ status: "confirm-downgrade" });
    else if (action.type === "back" && current.status === "confirm-downgrade") change({ status: "available" });
    else if (action.type === "confirm-downgrade") await download(true);
    else if (action.type === "download") await download();
    else if (action.type === "install") await install();
  }

  async function actionCancel() { await action({ type: "cancel" }); }
  const actionRef = useRef(action);
  actionRef.current = action;

  useEffect(() => {
    active.current = true;
    if (!isTauri()) return;
    let disposed = false;
    const subscription = listen<UpdateAction>("update-prompt-action", (event) => { void actionRef.current(event.payload); });
    void subscription.then(() => { if (!disposed) void checkForUpdates(false); })
      .catch((error) => { if (!disposed) change({ error: `Could not listen for update actions: ${String(error)}` }); });
    return () => {
      disposed = true;
      active.current = false;
      ++request.current;
      void subscription.then((unlisten) => unlisten()).catch(console.error);
      if (!working.current) void closeUpdate();
    };
  }, []);

  useEffect(() => {
    if (checkRequest > 0 && active.current) {
      if (stateRef.current.status === "ready") showWindow();
      else void checkForUpdates(true);
    }
  }, [checkRequest]);

  useEffect(() => { if (isTauri()) void syncWindow(); }, [state, canInstall]);
  const label = state.status === "ready" ? "Update ready" : state.status === "downloading" ? "Downloading update…"
    : state.status === "installing" ? "Installing update…" : state.version ? "Update available" : "Check for updates…";
  useEffect(() => {
    if (isTauri()) void invoke("set_update_menu_state", { text: label, enabled: state.status !== "installing" })
      .catch((error) => { if (active.current) change({ error: `Could not update the update menu: ${String(error)}` }); });
  }, [label, state.status]);

  if (!isTauri()) return null;
  return <Dialog open={fallback} onOpenChange={(open) => { if (!open) void actionCancel(); }}>
    <DialogContent className="update-fallback"><DialogTitle className="sr-only">Scorewing updates</DialogTitle>
      <UpdatePanel dialogTitle state={{ ...view, error: [windowError, state.error].filter(Boolean).join(" ") }} onAction={(value) => { void action(value); }} />
    </DialogContent>
  </Dialog>;
}
