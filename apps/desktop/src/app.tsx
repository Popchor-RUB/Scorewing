import { createRoot } from "react-dom/client";
import { Button } from "./components/ui/button.tsx";
import { SplitWorkspace } from "./split-workspace.tsx";
import { MergeWorkspace } from "./merge-workspace.tsx";
import { DiffWorkspace } from "./diff-workspace.tsx";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { relaunch } from "@tauri-apps/plugin-process";
import type { Update } from "@tauri-apps/plugin-updater";
import {
  addGroup, chooseArrangementTool, newArrangement, newPractice, operation,
  practiceOptions, orderScorePair,
  type Arrangement, type Inspection, type PracticeConfig,
  type PracticeEnvironment, type PracticeOptions, type PracticeProgress, type PracticeResult,
  type PracticeConflict, type PracticeOutputChoice,
} from "./model.ts";
import { PracticeView } from "./practice-view.tsx";
import { PracticeExportModal } from "./practice-export-modal.tsx";
import { PracticeConflictModal } from "./practice-conflict-modal.tsx";
import { RecentSoundsProvider } from "./sound-picker.tsx";
import { readPracticeDefaults, savePracticeDefaults } from "./practice-defaults.ts";
import { readRecentScores, rememberScore } from "./recent-scores.ts";
import { RecentScoreButton, RecentScores } from "./recent-score-button.tsx";
import { DiffPairModal } from "./diff-pair-modal.tsx";
import { UpdateControl } from "./update-control.tsx";

interface ScoreSession {
  document: Inspection;
  stage: "selection" | "configuration";
  older?: Inspection;
  arrangement: Arrangement;
  practice?: PracticeOptions;
  progress?: PracticeProgress;
  result?: PracticeResult;
  warnings?: string[];
}

const modes = [
  { kind: "split", title: "Split instruments", description: "Separate voices or chord tones into staves.", icon: "M4 12h5m0 0 5-6h6M9 12l5 6h6M17 3l3 3-3 3m0 6 3 3-3 3" },
  { kind: "merge", title: "Merge instruments", description: "Combine instruments into a shared staff.", icon: "M4 6h5l5 6m-10 6h5l5-6m0 0h6m-3-3 3 3-3 3" },
  { kind: "practice", title: "Create Practice Tracks", description: "Create individual MP3 practice tracks.", icon: "M9 17V5l11-2v12M9 9l11-2M9 17c0 2-5 4-5 1s5-4 5-1m11-2c0 2-5 4-5 1s5-4 5-1" },
  { kind: "diff", title: "Diff versions", description: "Mark changes between two score versions.", icon: "M8 3H4v18h4m8-18h4v18h-4M9 8h6m-6 8h6m-3-3v6" },
] as const;

export function mountApp(container: HTMLElement): () => void {
  const root = createRoot(container);
  let current: ScoreSession | null = null;
  let focusTarget: "configuration" | Arrangement["kind"] | null = null;
  let fileBusy = false;
  let exporting = false;
  let installing = false;
  let updateInstalled = false;
  let cancelling = false;
  let dropping = false;
  let disposed = false;
  let message = "";
  let messageError = false;
  let completedExport: PracticeResult | null = null;
  let outputConflict: PracticeConflict | null = null;
  let pendingPair: [Inspection, Inspection] | null = null;
  const releasedPairIds = new Set<string>();
  let pairError = "";
  const config: PracticeConfig = { executable: null, soundfont: null };
  const practiceDefaults = readPracticeDefaults();
  let recentScores = readRecentScores();
  let environment: PracticeEnvironment | null = null;
  let checking = false;
  let updateCheckRequest = 0;
  let fileMenuState = "";
  let environmentRequest = 0;
  const unlisteners: UnlistenFn[] = [];
  const busy = () => fileBusy || exporting || installing;

  async function installUpdate(update: Update) {
    if (disposed || busy() || current || pendingPair || checking)
      throw new Error("Close your score and wait for current work to finish before installing.");
    installing = true;
    render();
    try {
      if (!updateInstalled) { await update.install(); updateInstalled = true; }
      await relaunch();
    } catch (error) { installing = false; render(); throw error; }
  }

  function notify(text: string, error = false) {
    if (disposed) return;
    message = text;
    messageError = error;
    render();
  }

  async function refreshEnvironment() {
    if (disposed || busy() || checking) return;
    const request = ++environmentRequest;
    checking = true;
    render();
    try {
      const detected = await invoke<PracticeEnvironment>("practice_environment", { config: { ...config } });
      if (!disposed && request === environmentRequest) environment = detected;
    } catch (error) {
      if (!disposed && request === environmentRequest)
        environment = { ...config, version: null, presets: [], error: String(error) };
    } finally {
      if (!disposed && request === environmentRequest) { checking = false; render(); }
    }
  }

  async function release(id: string) {
    await invoke("release_document", { id });
  }

  async function releaseSession(session: ScoreSession) {
    if (session.older) {
      await release(session.older.id);
      session.older = undefined;
    }
    await release(session.document.id);
  }

  function releaseAfterDispose() {
    if (!disposed) return;
    if (pendingPair) {
      const pair = pendingPair;
      pendingPair = null;
      pair.filter((score) => !releasedPairIds.has(score.id)).forEach((score) => { void release(score.id).catch(console.error); });
    }
    if (!current) return;
    const session = current;
    current = null;
    void releaseSession(session).catch(console.error);
  }

  async function prepare(score: Inspection): Promise<Inspection> {
    if (score.format !== "musescore") return score;
    const prepared = await invoke<Inspection>("prepare_musicxml", { id: score.id, executable: config.executable ?? undefined });
    if (prepared.error) throw new Error(prepared.error);
    return { ...prepared, warnings: [...new Set([...(score.warnings ?? []), ...(prepared.warnings ?? [])])] };
  }

  async function inspectPair(paths: string[]) {
    const candidates: Inspection[] = [];
    try {
      for (const path of paths) {
        const score = await invoke<Inspection>("inspect_file", { path });
        candidates.push(score);
        if (score.error) throw new Error(score.error);
        if (disposed) return;
      }
      pendingPair = orderScorePair(candidates[0], candidates[1]);
      releasedPairIds.clear();
      candidates.length = 0;
      pairError = "";
      message = "";
    } catch (error) { notify(`Could not open scores: ${String(error)}`, true); }
    finally {
      const results = await Promise.allSettled(candidates.map((score) => release(score.id)));
      const failed = results.find((result) => result.status === "rejected");
      if (failed?.status === "rejected") notify(`Could not release an unused score: ${String(failed.reason)}`, true);
    }
  }

  async function releasePair() {
    if (!pendingPair) return;
    const results = await Promise.allSettled(pendingPair.filter((score) => !releasedPairIds.has(score.id)).map(async (score) => {
      await release(score.id);
      releasedPairIds.add(score.id);
    }));
    const failed = results.find((result) => result.status === "rejected");
    if (failed?.status === "rejected") throw failed.reason;
    pendingPair = null;
    releasedPairIds.clear();
  }

  async function cancelPair() {
    if (!pendingPair || busy()) return;
    fileBusy = true;
    render();
    try {
      await releasePair();
      pairError = "";
      message = "";
    } catch (error) { pairError = `Could not close comparison: ${String(error)}`; }
    finally { fileBusy = false; if (disposed) releaseAfterDispose(); render(); }
  }

  async function exportDiff() {
    if (releasedPairIds.size || busy() || disposed) return;
    const pair = pendingPair;
    const session = current;
    const older = pair?.[0] ?? session?.older;
    const newer = pair?.[1] ?? session?.document;
    if (!older || !newer) return;
    exporting = true;
    pairError = "";
    let exported = "";
    notify("Choose where to save your marked newer version.");
    try {
      const preparedOlder = await prepare(older);
      if (disposed) return;
      const preparedNewer = await prepare(newer);
      if (disposed) return;
      if (pair) pendingPair = [preparedOlder, preparedNewer];
      else if (session) { session.older = preparedOlder; session.document = preparedNewer; }
      const result = await invoke<{ path: string; warnings: string[] } | null>("export_document", {
        id: preparedNewer.id, operation: { kind: "diff", older_id: preparedOlder.id },
      });
      if (disposed) return;
      if (!result) { notify("Export cancelled."); return; }
      exported = [`Exported ${result.path}`, ...(result.warnings.length ? ["Review warnings:", ...result.warnings] : [])].join("\n");
      recentScores = rememberScore(rememberScore(recentScores, older.path), newer.path);
      await releasePair();
      if (current) { await releaseSession(current); current = null; }
      if (disposed) return;
      completedExport = null;
      window.scrollTo(0, 0);
      notify(exported);
    } catch (error) {
      const detail = exported ? `${exported}\nCould not close comparison: ${String(error)}` : String(error);
      if (pendingPair) pairError = detail;
      else notify(detail, true);
    } finally { exporting = false; if (disposed) releaseAfterDispose(); render(); }
  }

  async function replaceScore(path: string) {
    let candidate: Inspection | undefined;
    try {
      candidate = await invoke<Inspection>("inspect_file", { path });
      if (candidate.error) throw new Error(candidate.error);
      if (disposed) return;
      if (current) {
        await releaseSession(current);
        current = null;
      }
      if (disposed) return;
      const arrangement = newArrangement();
      if (candidate.format === "musescore") arrangement.kind = "practice";
      current = {
        document: candidate, stage: "selection", arrangement,
        practice: candidate.format === "musescore" ? newPractice(candidate.practice_parts ?? [], practiceDefaults, candidate.practice_sections ?? []) : undefined,
      };
      completedExport = null;
      recentScores = rememberScore(recentScores, candidate.path);
      candidate = undefined;
      message = "";
      messageError = false;
      window.scrollTo(0, 0);
    } catch (error) {
      notify(`Could not open score: ${String(error)}`, true);
    } finally {
      if (candidate) {
        try { await release(candidate.id); }
        catch (error) { notify(`Could not release the unused score: ${String(error)}`, true); }
      }
    }
  }

  async function openScore(paths?: string[]) {
    if (disposed) return;
    if (busy() || pendingPair) { notify("Finish the current operation before opening another score.", true); return; }
    fileBusy = true;
    render();
    try {
      const selected = paths ?? await invoke<string[] | null>("open_files");
      if (!selected || disposed) return;
      if (selected.length < 1 || selected.length > 2) { notify("Choose one score to open or two scores to compare.", true); return; }
      if (selected.length === 2) await inspectPair(selected);
      else await replaceScore(selected[0]);
    } catch (error) { notify(String(error), true); }
    finally {
      fileBusy = false;
      if (disposed) releaseAfterDispose();
      render();
      if (!disposed && current?.practice && !environment && !checking) void refreshEnvironment();
    }
  }

  async function closeScore() {
    if (busy() || pendingPair || !current || disposed) return;
    fileBusy = true;
    render();
    try {
      await releaseSession(current);
      current = null;
      message = "";
      completedExport = null;
      messageError = false;
      window.scrollTo(0, 0);
    } catch (error) { notify(`Could not close score: ${String(error)}`, true); }
    finally { fileBusy = false; if (disposed) releaseAfterDispose(); render(); }
  }

  async function chooseDiffFile(version: "old" | "new") {
    if ((!current && !pendingPair) || busy() || releasedPairIds.size || disposed) return;
    const pair = pendingPair;
    const session = current;
    let candidate: Inspection | undefined;
    fileBusy = true;
    render();
    try {
      const path = await invoke<string | null>("open_file");
      if (!path || disposed) return;
      candidate = await invoke<Inspection>("inspect_file", { path });
      if (candidate.error) throw new Error(candidate.error);
      if (!pair) candidate = await prepare(candidate);
      if (disposed) return;
      const previous = pair ? pair[version === "old" ? 0 : 1] : version === "old" ? session!.older : session!.document;
      if (previous) await release(previous.id);
      if (pair) pair[version === "old" ? 0 : 1] = candidate;
      else if (version === "old") session!.older = candidate;
      else setDiffNewer(session!, candidate);
      candidate = undefined;
      pairError = "";
      message = "";
      messageError = false;
    } catch (error) {
      const detail = `Could not open ${version === "old" ? "older" : "newer"} version: ${String(error)}`;
      if (pair) pairError = detail;
      else notify(detail, true);
    }
    finally {
      if (candidate) {
        try { await release(candidate.id); }
        catch (error) { notify(`Could not release the unused score: ${String(error)}`, true); }
      }
      fileBusy = false;
      if (disposed) releaseAfterDispose();
      render();
    }
  }

  function setDiffNewer(session: ScoreSession, score: Inspection) {
    session.document = score;
    session.arrangement = newArrangement();
    session.arrangement.kind = "diff";
    session.practice = score.format === "musescore" ? newPractice(score.practice_parts ?? [], practiceDefaults, score.practice_sections ?? []) : undefined;
    session.result = undefined;
    session.progress = undefined;
    session.warnings = undefined;
    completedExport = null;
  }

  function swapDiffFiles() {
    if (busy() || releasedPairIds.size || disposed) return;
    if (pendingPair) pendingPair = [pendingPair[1], pendingPair[0]];
    else if (current?.older) {
      const older = current.older;
      current.older = current.document;
      setDiffNewer(current, older);
    }
    pairError = "";
    message = ""; messageError = false;
    render();
  }

  async function changeOperation(kind: Arrangement["kind"]) {
    if (!current || busy() || disposed) return;
    const session = current;
    if (kind === "practice" && !session.practice) return;
    fileBusy = true;
    render();
    try {
      if (kind !== "practice" && kind !== "diff") session.document = await prepare(session.document);
      if (disposed) return;
      chooseArrangementTool(session.arrangement, kind, session.document.parts, session.document.format === "musescore");
      session.stage = "configuration";
      focusTarget = "configuration";
      message = ""; messageError = false;
    } catch (error) { notify(`Could not prepare score: ${String(error)}`, true); }
    finally { fileBusy = false; if (disposed) releaseAfterDispose(); render(); }
    if (kind === "practice" && !environment && !checking) void refreshEnvironment();
  }

  function backToModes() {
    if (!current || busy() || disposed) return;
    current.stage = "selection";
    focusTarget = current.arrangement.kind;
    render();
  }

  async function browse(kind: keyof PracticeConfig) {
    if (busy() || checking || disposed) return;
    fileBusy = true;
    render();
    let changed = false;
    try {
      const path = await invoke<string | null>("pick_practice_path", { kind });
      if (path && !disposed) {
        config[kind] = path;
        environment = null;
        changed = true;
      }
    } catch (error) { notify(String(error), true); }
    finally { fileBusy = false; if (disposed) releaseAfterDispose(); render(); }
    if (changed) await refreshEnvironment();
  }

  async function exportScore() {
    if (current?.arrangement.kind === "diff") { await exportDiff(); return; }
    if (!current || busy() || disposed || (current.arrangement.kind === "practice" && checking)) return;
    const session = current;
    try {
      const options = session.arrangement.kind === "practice" && session.practice ? practiceOptions(session.practice, environment?.presets ?? []) : null;
      const request = options ? null : operation(session.arrangement, session.document.parts, session.older?.id);
      if (options && (!environment?.executable || !environment.soundfont || environment.error))
        throw new Error(environment?.error ?? "Configure MuseScore 4.7.x and its MS Basic SoundFont before rendering.");
      exporting = true;
      cancelling = false;
      completedExport = null;
      session.result = undefined;
      session.progress = undefined;
      notify(options ? "Choose a folder for your practice tracks." : "Choose where to save your arranged score.");
      if (options) {
        const result = await invoke<PracticeResult | null>("export_practice_tracks", {
          id: session.document.id, options, config: { ...config },
        });
        if (!disposed && current === session) {
          if (result && !result.cancelled && !result.error) {
            completedExport = result;
            notify("Practice tracks created.");
          } else {
            session.result = result ?? undefined;
            notify(result ? `${result.cancelled ? "Rendering cancelled." : "Rendering stopped."} ${result.completed.length} completed tracks retained in ${result.directory}${result.error ? `\n${result.error}` : ""}` : "Export cancelled.", !!result?.error);
          }
        }
      } else {
        const result = await invoke<{ path: string; warnings: string[] } | null>("export_document", {
          id: session.document.id, operation: request,
        });
        if (!disposed && current === session) {
          if (result) session.warnings = [...new Set([...(session.document.warnings ?? []), ...result.warnings])];
          notify(result ? `Exported ${result.path}${session.warnings?.length ? "\nReview warnings below." : ""}` : "Export cancelled.");
        }
      }
    } catch (error) { notify(String(error), true); }
    finally {
      exporting = false; cancelling = false; outputConflict = null; session.progress = undefined;
      if (disposed) releaseAfterDispose();
      render();
    }
  }

  async function cancelExport() {
    if (!current?.practice || current.arrangement.kind !== "practice" || !exporting || cancelling || disposed) return;
    const session = current;
    cancelling = true;
    render();
    try {
      await invoke("cancel_practice_export", { id: session.document.id });
      if (exporting && current === session) notify("Cancelling rendering… Completed tracks will be retained.");
    } catch (error) {
      if (exporting && current === session) { cancelling = false; notify(`Could not cancel rendering: ${String(error)}`, true); }
    }
  }

  async function resolveOutputConflict(choice: PracticeOutputChoice) {
    const conflict = outputConflict;
    if (!conflict || disposed) return;
    await invoke("resolve_practice_export", { id: conflict.id, token: conflict.token, choice });
    if (outputConflict === conflict) { outputConflict = null; render(); }
  }

  function update(change: (session: ScoreSession) => void) {
    if (!current || busy() || disposed) return;
    change(current);
    render();
  }

  function render() {
    if (disposed) return;
    const session = current;
    const score = session?.document;
    const museScore = score?.format === "musescore";
    const openEnabled = !busy() && !pendingPair;
    const closeEnabled = openEnabled && !!session;
    const menuState = `${openEnabled}:${closeEnabled}`;
    if (isTauri() && menuState !== fileMenuState) {
      fileMenuState = menuState;
      void invoke("set_file_menu_state", { openEnabled, closeEnabled })
        .catch((error) => notify(`Could not update the File menu: ${String(error)}`, true));
    }
    const configured = session?.stage === "configuration";
    const tool = modes.find(({ kind }) => kind === session?.arrangement.kind);
    const count = museScore && (!configured || session.arrangement.kind === "practice") ? (score!.practice_parts?.length ?? score!.parts.length) : (score?.parts.length ?? 0);
    const warnings = [...new Set([...(score?.warnings ?? []), ...(session?.warnings ?? [])])];
    root.render(<RecentSoundsProvider><div className="app-shell">
      <header className="app-header">
        {configured ? <div className="tool-heading">
          <Button variant="ghost" data-action="back-to-modes" disabled={busy()} onClick={backToModes}><span aria-hidden="true">←</span> All tools</Button>
          <span className="toolbar-separator" aria-hidden="true" />
          <h2 id="mode-config-heading" tabIndex={-1}
            ref={(node) => { if (node && focusTarget === "configuration") { focusTarget = null; node.focus(); } }}>{tool!.title}</h2>
        </div> : session ? <h1>Choose a tool</h1> : <RecentScoreButton paths={recentScores} disabled={busy()} reading={fileBusy} onOpen={(paths) => void openScore(paths)} />}
        {session ? <Button variant="outline" data-action="open" disabled={!openEnabled} onClick={() => void openScore()}><svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="M3 6h6l2 3h10v12H3Z M3 6V3h6l2 3h10v3" /></svg> Open score</Button>
          : <span className="shortcut muted">{/Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl"} + O</span>}
      </header>
      <UpdateControl checkRequest={updateCheckRequest} canInstall={!session && !pendingPair && !busy() && !checking} onInstall={installUpdate} />
      {session && <div className="score-heading">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true" className="file-icon"><path d="M14 2H5v20h14V7z M14 2v5h5 M8 12h8 M8 16h8" /></svg><strong title={score!.path}>{score!.name}</strong>
        <span className="muted">{count} {count === 1 ? "instrument" : "instruments"}</span>
        <Button variant="ghost" data-action="close" disabled={!closeEnabled} title="Closing this score discards its editing choices." onClick={() => void closeScore()}>Close</Button>
      </div>}
      <main className="workspace"><section className="editor" aria-label="Score arrangement">
        {!session ? <div className="empty">
          <RecentScores paths={recentScores} disabled={busy()} onOpen={(paths) => void openScore(paths)} />
          <div className="drop-hint"><svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" className="drop-icon" aria-hidden="true"><path d="M3 6h6l2 3h10v12H3Z M3 6V3h6l2 3h10v3" /></svg><div><strong>Drop a score here to open it</strong><p className="muted">Drop two versions to compare them.</p></div></div>
        </div> : <div className="score-workspace" key={score!.id}>
          {session.stage === "selection" ? <section className="mode-selection" aria-labelledby="mode-selection-heading">
            <div className="mode-intro"><h2 id="mode-selection-heading">What would you like to do?</h2><p className="muted">Choose a tool for the open score.</p></div>
            <div className="mode-grid">
              {modes.map(({ kind, title, description, icon }) => <Button key={kind} variant="outline" className="mode-tile" data-mode={kind}
                aria-labelledby={`mode-${kind}-title`} aria-describedby={`mode-${kind}-description`}
                disabled={busy() || (kind === "practice" && !session.practice)} onClick={() => void changeOperation(kind)}
                ref={(node) => { if (node && focusTarget === kind) { focusTarget = null; node.focus(); } }}>
                <span className="mode-icon" aria-hidden="true"><svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><path d={icon} /></svg></span>
                <span id={`mode-${kind}-title`} className="mode-title">{title}</span><span className="mode-arrow muted" aria-hidden="true">→</span>
                <span id={`mode-${kind}-description`} className="mode-description">{kind === "practice" && !session.practice ? "Requires a MuseScore .mscz or .mscx score." : description}</span>
              </Button>)}
            </div>
            {session.practice && <p className="muted">Split and merge convert all MuseScore instruments to MusicXML, including hidden sources.</p>}
          </section> : session.arrangement.kind === "practice" && session.practice ? <PracticeView parts={score!.practice_parts ?? []} sections={score!.practice_sections ?? []} options={session.practice} config={config} environment={environment} checking={checking}
            busy={busy()} rendering={exporting} result={session.result}
            defaults={practiceDefaults} onSetDefault={(mode, sound) => update(({ practice }) => {
              practiceDefaults[mode] = sound;
              practice!.parts.forEach((part) => { part[mode] = sound; });
              savePracticeDefaults(practiceDefaults);
            })} onSetLevelDefault={(field) => update(({ practice }) => {
              practiceDefaults[field] = practice![field];
              savePracticeDefaults(practiceDefaults);
            })} onOptionsChange={() => {
              practiceDefaults.metronome_enabled = session.practice!.metronome_enabled;
              savePracticeDefaults(practiceDefaults);
              render();
            }}
            onConfigChange={(kind, value) => {
              if (busy()) return;
              config[kind] = value.trim() || null; environment = null; ++environmentRequest; checking = false; render();
            }} onBrowse={(kind) => void browse(kind)} onDetect={() => void refreshEnvironment()} onExport={() => void exportScore()} />
          : session.arrangement.kind === "diff" ? <DiffWorkspace older={session.older} newer={session.document} busy={busy()} exporting={exporting}
              onChoose={(version) => void chooseDiffFile(version)} onSwap={swapDiffFiles} onExport={() => void exportScore()} />
          : session.arrangement.kind === "split" ? <SplitWorkspace parts={score!.parts} state={session.arrangement} busy={busy()} exporting={exporting}
              onChange={(change) => update(({ arrangement }) => change(arrangement))} onExport={() => void exportScore()} />
          : <MergeWorkspace parts={score!.parts} state={session.arrangement} busy={busy()} exporting={exporting}
              onChange={(change) => update(({ arrangement }) => change(arrangement))} onExport={() => void exportScore()}
              onCreateGroup={() => update(({ arrangement }) => { const error = addGroup(arrangement, score!.parts); message = error ?? "Merge group created. Reorder its instruments if needed."; messageError = !!error; })} />}
        </div>}
      </section></main>
      {configured && (session.arrangement.kind === "split" || session.arrangement.kind === "merge") && warnings.length > 0 && <section className="conversion-warnings" role="status" aria-live="polite" aria-atomic="true" aria-labelledby="conversion-warnings-heading" tabIndex={0}>
        <h3 id="conversion-warnings-heading">Review warnings</h3>
        <p>Review these details in the exported score.</p>
        <ul>{warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul>
      </section>}
      <footer className="status-bar">
        <div className={`status ${messageError ? "text-danger" : "muted"}`} role={messageError ? "alert" : "status"} aria-live="polite">{message}</div>
        <span className="status-context muted">{session ? "Source files are preserved" : "MusicXML · .mxl · MuseScore .mscz / .mscx"}</span>
      </footer>
      {outputConflict && <PracticeConflictModal key={outputConflict.token} conflict={outputConflict} onResolve={resolveOutputConflict} />}
      {!outputConflict && (completedExport || (exporting && session?.arrangement.kind === "practice")) && <PracticeExportModal result={completedExport}
        progress={session?.progress} total={session?.practice
          ? session.practice.parts.filter((part) => part.create_track).length * (Number(session.practice.include_whole_song) + session.practice.section_keys.length) : 0}
        cancelling={cancelling} onCancel={() => void cancelExport()} onClose={() => { completedExport = null; render(); }} />}
      {pendingPair && <DiffPairModal pair={pendingPair} busy={busy()} closing={releasedPairIds.size > 0} error={pairError} status={message} onChoose={(version) => void chooseDiffFile(version)} onSwap={swapDiffFiles} onCancel={() => void cancelPair()} onCompare={() => void exportDiff()} />}
      {dropping && <div className="drop-overlay" aria-hidden="true"><div><p>Drop one score to open it or two to compare</p><p className="muted">MusicXML · MuseScore</p></div></div>}
    </div></RecentSoundsProvider>);
  }

  const preventNavigation = (event: Event) => event.preventDefault();
  window.addEventListener("dragover", preventNavigation);
  window.addEventListener("drop", preventNavigation);
  function register(promise: Promise<UnlistenFn>) {
    void promise.then((unlisten) => { if (disposed) unlisten(); else unlisteners.push(unlisten); })
      .catch((error) => notify(`Native events are unavailable: ${String(error)}. Use Open score.`, true));
  }
  register(listen<PracticeProgress>("practice-progress", (event) => {
    if (!disposed && exporting && current?.practice && current.document.id === event.payload.id) { current.progress = event.payload; render(); }
  }));
  register(listen<PracticeConflict>("practice-export-conflict", (event) => {
    if (!disposed && exporting && current?.practice && current.document.id === event.payload.id) {
      outputConflict = event.payload;
      render();
    }
  }));
  register(listen<string>("menu-action", (event) => {
    if (disposed) return;
    if (event.payload === "open-score") void openScore();
    else if (event.payload === "close-score") void closeScore();
    else if (event.payload === "check-updates" && !installing) { ++updateCheckRequest; render(); }
  }));
  try {
    register(getCurrentWebviewWindow().onDragDropEvent((event) => {
      if (disposed) return;
      dropping = event.payload.type === "enter" || event.payload.type === "over";
      render();
      if (event.payload.type === "drop") void openScore(event.payload.paths);
    }));
  } catch (error) { notify(`Native file dropping is unavailable: ${String(error)}. Use Open score.`, true); }
  render();
  return () => {
    disposed = true;
    ++environmentRequest;
    window.removeEventListener("dragover", preventNavigation);
    window.removeEventListener("drop", preventNavigation);
    unlisteners.forEach((unlisten) => unlisten());
    if (exporting && current?.practice && !current.progress) {
      void invoke("cancel_practice_export", { id: current.document.id }).catch(() => {});
    }
    // Active exports own their session until completion; never release it underneath the renderer.
    if (!exporting && !fileBusy) releaseAfterDispose();
    root.unmount();
  };
}
