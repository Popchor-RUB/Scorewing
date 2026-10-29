import { createRoot } from "react-dom/client";
import { Button, Card, Checkbox, Chip, Description, Label, ListBox, Select, Surface } from "@heroui/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  addGroup, fallbackVoice, movePart, newArrangement, newPractice, operation,
  practiceOptions, setFallbackVoice, stemDirection, orderScorePair,
  type Arrangement, type Inspection, type Part, type PracticeConfig,
  type PracticeEnvironment, type PracticeOptions, type PracticeProgress, type PracticeResult,
} from "./model.ts";
import { PracticeView } from "./practice-view.tsx";
import { PracticeExportModal } from "./practice-export-modal.tsx";
import { RecentSoundsProvider } from "./sound-picker.tsx";
import { readPracticeDefaults, savePracticeDefaults } from "./practice-defaults.ts";
import { readRecentScores, rememberScore } from "./recent-scores.ts";
import { RecentScoreButton } from "./recent-score-button.tsx";
import { DiffFileSelectors, DiffPairModal } from "./diff-pair-modal.tsx";

interface ScoreSession {
  document: Inspection;
  stage: "selection" | "configuration";
  older?: Inspection;
  arrangement: Arrangement;
  practice?: PracticeOptions;
  progress?: PracticeProgress;
  result?: PracticeResult;
}

const modes = [
  { kind: "split", title: "Split instruments", description: "Give every line its own staff.", icon: "M4 12h5m0 0 5-6h6M9 12l5 6h6M17 3l3 3-3 3m0 6 3 3-3 3" },
  { kind: "merge", title: "Merge instruments", description: "Bring parts together in a single staff.", icon: "M4 6h5l5 6m-10 6h5l5-6m0 0h6m-3-3 3 3-3 3" },
  { kind: "diff", title: "Diff versions", description: "See what changed between two scores.", icon: "M8 3H4v18h4m8-18h4v18h-4M9 8h6m-6 8h6m-3-3v6" },
  { kind: "practice", title: "Create Practice Tracks", description: "Let your instrument take the foreground.", icon: "M9 17V5l11-2v12M9 9l11-2M9 17c0 2-5 4-5 1s5-4 5-1m11-2c0 2-5 4-5 1s5-4 5-1" },
] as const;

function Choice({ id, label, value, options, disabled, onChange }: {
  id: string; label: string; value: string;
  options: { key: string; label: string }[]; disabled: boolean;
  onChange: (value: string) => void;
}) {
  return <Select variant="secondary" id={id} className="score-select min-w-0 w-full" isDisabled={disabled} value={value}
    onChange={(key) => { if (key !== null) onChange(String(key)); }}>
    <Label>{label}</Label>
    <Select.Trigger><Select.Value /><Select.Indicator /></Select.Trigger>
    <Select.Popover><ListBox items={options}>{(option) =>
      <ListBox.Item id={option.key} textValue={option.label}>{option.label}<ListBox.ItemIndicator /></ListBox.Item>
    }</ListBox></Select.Popover>
  </Select>;
}

function SplitPreview({ part, state, busy, onChange }: {
  part: Part; state: Arrangement; busy: boolean; onChange: () => void;
}) {
  const lanes = part.split_plans[state.mode];
  const voices = [...new Set([...part.split_plans.voices, ...lanes].map((lane) => lane.voice))];
  return <section className="split-preview" aria-label={`Split preview for ${part.name || part.id} ${part.id}`}><Surface className="fallback-panel">
    <div className="fallback-header"><h4 className="font-semibold">Fallback voices</h4><span className="text-xs text-muted">{lanes.length} additional {lanes.length === 1 ? "staff" : "staves"}</span></div>
    <Description className="fallback-help">Used only when a staff’s own voices are absent for a whole measure.</Description>
    <div className="fallback-rows">{lanes.map((lane, li) => <div className="fallback-row" key={`${lane.voice}:${lane.tone}`} role="group" aria-labelledby={`staff-${part.id}-${lane.voice}-${lane.tone}`}>
      <div className="fallback-staff"><h5 id={`staff-${part.id}-${lane.voice}-${lane.tone}`}>Additional staff {li + 1}</h5>
        <Description>Voice {lane.voice}{state.mode === "chords" && <> · Chord tone {lane.tone + 1}</>}</Description>
      </div>
      <Choice id={`fallback-${part.id}-${lane.voice}-${lane.tone}`} label="Fallback voice"
        value={fallbackVoice(state, part.id, lane) === null ? "none" : `voice:${fallbackVoice(state, part.id, lane)}`}
        options={[{ key: "none", label: "None (silent)" }, ...voices.filter((voice) => voice !== lane.voice).map((voice) => ({ key: `voice:${voice}`, label: `Voice ${voice}` }))]}
        disabled={busy} onChange={(value) => { setFallbackVoice(state, part.id, lane, value === "none" ? null : value.slice(6)); onChange(); }} />
    </div>)}</div>
    {state.mode === "chords" && <Description className="fallback-note">Chord-tone staves can also carry compatible voices in measures without that chord tone.</Description>}
  </Surface></section>;
}

export function mountApp(container: HTMLElement): () => void {
  const root = createRoot(container);
  let current: ScoreSession | null = null;
  let focusTarget: "configuration" | Arrangement["kind"] | null = null;
  let fileBusy = false;
  let exporting = false;
  let cancelling = false;
  let dropping = false;
  let disposed = false;
  let message = "";
  let messageError = false;
  let completedExport: PracticeResult | null = null;
  let pendingPair: [Inspection, Inspection] | null = null;
  const releasedPairIds = new Set<string>();
  let pairError = "";
  const config: PracticeConfig = { executable: null, soundfont: null };
  const practiceDefaults = readPracticeDefaults();
  let recentScores = readRecentScores();
  let environment: PracticeEnvironment | null = null;
  let checking = false;
  let environmentRequest = 0;
  const unlisteners: UnlistenFn[] = [];
  const busy = () => fileBusy || exporting;

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
    return prepared;
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
        practice: candidate.format === "musescore" ? newPractice(candidate.practice_parts ?? [], practiceDefaults) : undefined,
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
    if (busy() || !current || disposed) return;
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
    session.practice = score.format === "musescore" ? newPractice(score.practice_parts ?? [], practiceDefaults) : undefined;
    session.result = undefined;
    session.progress = undefined;
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
      if (session.arrangement.kind !== kind) session.arrangement.selected.clear();
      session.arrangement.kind = kind;
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
          session.result = result ?? undefined;
          if (result && !result.cancelled && !result.error) completedExport = result;
          notify(result ? `${result.cancelled ? "Rendering cancelled." : result.error ? "Rendering stopped." : "Practice tracks created."} ${result.completed.length} completed tracks retained in ${result.directory}${result.error ? `\n${result.error}` : ""}` : "Export cancelled.", !!result?.error);
        }
      } else {
        const result = await invoke<{ path: string; warnings: string[] } | null>("export_document", {
          id: session.document.id, operation: request,
        });
        if (!disposed && current === session) notify(result ? [
          `Exported ${result.path}`,
          ...(result.warnings.length ? ["Review warnings — affected passages are marked red:", ...result.warnings] : []),
        ].join("\n") : "Export cancelled.");
      }
    } catch (error) { notify(String(error), true); }
    finally {
      exporting = false; cancelling = false; session.progress = undefined;
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

  function update(change: (session: ScoreSession) => void) {
    if (!current || busy() || disposed) return;
    change(current);
    render();
  }

  function arrangementView(session: ScoreSession) {
    const { document: score, arrangement: state } = session;
    const assigned = new Set(state.groups.flat());
    const available = score.parts.filter((part) => state.kind !== "merge" || !assigned.has(part.id));
    const allSelected = available.length > 0 && available.every((part) => state.selected.has(part.id));
    return <>
      <Card className="arrangement-card">
        <Card.Header className="section-heading flex-row flex-wrap items-center justify-between gap-4"><div className="grid min-w-0 gap-2"><Description>01 / INSTRUMENTS</Description>
          <Card.Title>{state.kind === "split" ? "Make room for every voice." : "Bring your parts together."}</Card.Title>
          <Card.Description>{state.kind === "split" ? "Choose instruments to split. Originals stay in place; new staves appear below them." : "Choose two or more instruments for a group. Other parts stay as they are."}</Card.Description>
        </div><Button variant="secondary" data-action="select-all" isDisabled={busy() || !available.length} onPress={() => update(({ arrangement }) => {
          arrangement.selected = allSelected ? new Set() : new Set(available.map((part) => part.id));
        })}>{allSelected ? "Clear selection" : "Select all"}</Button></Card.Header>
        <Card.Content><div className="instruments grid gap-4" role="group" aria-label="Instruments">
          {score.parts.map((part, index) => <div className="instrument-entry" key={part.id}>
            <Checkbox variant="secondary" className="instrument w-full" data-part={part.id} isSelected={state.selected.has(part.id)}
              isDisabled={busy() || (state.kind === "merge" && assigned.has(part.id))}
              onChange={(selected) => update(({ arrangement }) => { if (selected) arrangement.selected.add(part.id); else arrangement.selected.delete(part.id); })}>
              <Checkbox.Content className="w-full items-center gap-3 py-2"><Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                <span className="part-number hidden w-6 shrink-0 text-sm text-muted tabular-nums sm:inline" aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
                <Label className="part-name min-w-0 flex-1 wrap-anywhere"><strong>{part.name || part.id}</strong></Label>
                <span className="voice-count ml-auto shrink-0 text-sm text-muted">{state.kind === "merge" && assigned.has(part.id) ? "Grouped" : `${part.voice_count} ${part.voice_count === 1 ? "voice" : "voices"}`}</span>
              </Checkbox.Content>
            </Checkbox>
            {state.kind === "split" && state.selected.has(part.id) && <SplitPreview part={part} state={state} busy={busy()} onChange={render} />}
          </div>)}
        </div></Card.Content>
      </Card>
      {state.kind === "split" ? <Card className="options-card"><Card.Header><Description>02 / SPLIT SETTINGS</Description><Card.Title>A staff for each line.</Card.Title></Card.Header>
        <Card.Content className="settings-grid grid gap-6 md:grid-cols-2"><div className="space-y-3"><Choice id="split-mode" label="Split by" value={state.mode} disabled={busy()}
          options={[{ key: "chords", label: "Chord tones" }, { key: "voices", label: "Explicit voices" }]}
          onChange={(mode) => update(({ arrangement }) => { arrangement.mode = mode as Arrangement["mode"]; })} />
          <Description>{state.mode === "chords" ? "Separate simultaneous notes by pitch into individual staves." : "Separate the voices already written into the score."}</Description></div>
          <div className="space-y-3"><Checkbox variant="secondary" id="insert-fallback-slurs" isSelected={state.insertFallbackSlurs} isDisabled={busy()}
            onChange={(selected) => update(({ arrangement }) => { arrangement.insertFallbackSlurs = selected; })}>
            <Checkbox.Content><Checkbox.Control><Checkbox.Indicator /></Checkbox.Control><Label>Automatically insert slurs at fallback voice transitions</Label></Checkbox.Content>
          </Checkbox><Description>Matching pitches continue existing ties. When pitches differ, this option uses a slur and marks its endpoint notes red for review. When disabled, the fallback note becomes a rest of the same duration.</Description></div>
        </Card.Content></Card> : <Card className="options-card"><Card.Header className="section-heading flex-row flex-wrap items-center justify-between gap-4"><div className="grid min-w-0 gap-2"><Description>02 / MERGE GROUPS</Description><Card.Title>Arrange the voices.</Card.Title><Card.Description>Each group becomes one staff. Top to bottom sets its voice order.</Card.Description></div>
          <div className="merge-create"><Description>{state.selected.size} instruments selected</Description><Button data-action="add-group" variant="secondary" isDisabled={busy() || state.selected.size < 2} onPress={() => update(({ arrangement }) => {
            const error = addGroup(arrangement, score.parts);
            message = error ?? "Merge group created. Reorder its instruments if needed."; messageError = !!error;
          })}>Create merge group</Button></div></Card.Header>
          <Card.Content><div className="groups space-y-6">{state.groups.length === 0 && <Surface className="merge-empty"><strong>No merge groups yet</strong><Description>Select two or more instruments above, then choose “Create merge group”.</Description></Surface>}
            {state.groups.map((group, gi) => <section className="merge-group" aria-label={`Merge group ${gi + 1}`} key={gi}>
              <div className="merge-group-header"><div className="grid gap-1"><h4 className="font-semibold">Group {gi + 1}</h4><Description>{group.length} instruments in one staff</Description></div><Button size="sm" variant="ghost" data-action="remove-group" data-group={gi} isDisabled={busy()} onPress={() => update(({ arrangement }) => { arrangement.groups.splice(gi, 1); })}>Remove group</Button></div>
              <ol>{group.map((id, pi) => {
                const name = score.parts.find((part) => part.id === id)?.name || id;
                return <li className="merge-part" key={id}><div className="merge-instrument" role="group" aria-labelledby={`merge-part-${id}`}>
                  <div className="group-part"><h5 id={`merge-part-${id}`}>{name}</h5><Description>Voice {pi + 1}</Description></div>
                  <Choice id={`stem-${id}`} label="Stem direction" value={stemDirection(state, id, pi)}
                    disabled={busy()} options={[{ key: "up", label: "Up" }, { key: "down", label: "Down" }]}
                    onChange={(value) => update(({ arrangement }) => { arrangement.stems.set(id, value as "up" | "down"); })} />
                  <div className="merge-order"><span className="text-sm">Voice order</span><span className="reorder flex gap-2"><Button isIconOnly variant="secondary" aria-label={`Move ${name}, voice ${pi + 1}, up in group ${gi + 1}`} data-action="up" data-group={gi} data-index={pi} isDisabled={busy() || pi === 0}
                    onPress={() => update(({ arrangement }) => movePart(arrangement.groups[gi], pi, -1))}>↑</Button>
                    <Button isIconOnly variant="secondary" aria-label={`Move ${name}, voice ${pi + 1}, down in group ${gi + 1}`} data-action="down" data-group={gi} data-index={pi} isDisabled={busy() || pi === group.length - 1}
                      onPress={() => update(({ arrangement }) => movePart(arrangement.groups[gi], pi, 1))}>↓</Button></span></div>
                </div></li>;
              })}</ol>
            </section>)}
          </div></Card.Content>
        </Card>}
      <footer className="export-bar sticky bottom-4 z-10"><Surface className="grid justify-items-start gap-2 rounded-3xl p-4 shadow-surface">
        <Button data-action="export" isDisabled={busy()} onPress={() => void exportScore()}>{exporting ? "Exporting…" : "Export score"}<span aria-hidden="true"> →</span></Button>
        <Description>Your original score is not modified.</Description>
      </Surface></footer>
    </>;
  }

  function diffView(session: ScoreSession) {
    return <Card className="diff-card"><Card.Header>
      <Card.Title>See what changed.</Card.Title>
      <Card.Description>Export the newer version with changed notes, lyrics, dynamics, chord symbols, ties, and slurs marked red. Deleted content marks nearby surviving music.</Card.Description>
    </Card.Header><Card.Content className="space-y-5">
      <DiffFileSelectors older={session.older} newer={session.document} disabled={busy()} onChoose={(version) => void chooseDiffFile(version)} onSwap={swapDiffFiles} />
      <Description>Sound-equivalent pitches such as C♯ and D♭ compare as equal. Both source files stay intact.</Description>
      <Button data-action="export" isDisabled={busy() || !session.older} onPress={() => void exportScore()}>{exporting ? "Exporting…" : "Export marked newer version"}</Button>
    </Card.Content></Card>;
  }

  function render() {
    if (disposed) return;
    const session = current;
    const score = session?.document;
    const museScore = score?.format === "musescore";
    root.render(<RecentSoundsProvider><div className="app-shell min-h-dvh bg-background text-foreground">
      <header className="app-header mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-4 p-5 sm:p-7"><div className="brand flex items-center gap-3"><span aria-hidden="true">♫</span><div className="flex flex-col gap-1"><strong>Voiceweaver</strong><Description>A little room to rearrange.</Description></div></div>
        <div className="file-actions flex flex-wrap gap-3">{session && <Button variant="ghost" data-action="close" isDisabled={busy()} onPress={() => void closeScore()}>Close score</Button>}
          <Button data-action="open" variant={session ? "secondary" : "primary"} isDisabled={busy()} onPress={() => void openScore()}>{fileBusy ? "Opening…" : session ? "Open another score" : "Open score"}<span aria-hidden="true"> ↗</span></Button>
        </div>
      </header>
      <main className="workspace mx-auto max-w-6xl p-5 sm:p-7"><section className="editor min-w-0" aria-label="Score arrangement">
        {!session ? <div className="empty flex min-h-96 flex-col items-center justify-center gap-6 py-12 text-center"><div className="text-6xl" aria-hidden="true">♫</div><Description>MAKE SPACE FOR YOUR MUSIC</Description><h1 className="text-4xl font-semibold sm:text-6xl">Every voice.<br />A little clearer.</h1>
          <p className="max-w-md text-muted">Split a staff, bring parts together, or make a practice track that lets your instrument shine.</p>
          <RecentScoreButton paths={recentScores} disabled={busy()} reading={fileBusy} onOpen={(paths) => void openScore(paths)} />
          <div className="drop-hint flex flex-col gap-1"><Description>Drop one score to open it, or drop or select two scores to compare versions.</Description><Description>MusicXML · .mxl · MuseScore .mscz / .mscx — formats can be mixed</Description></div>
        </div> : <div className="score-workspace space-y-6" key={score!.id}>
          <div className="score-heading flex flex-wrap items-center justify-between gap-4"><div className="min-w-0 space-y-3"><Description>YOUR MUSIC, YOUR ARRANGEMENT</Description><h1 className="text-3xl font-semibold wrap-anywhere">{score!.name}</h1><Description>{museScore ? (score!.practice_parts?.length ?? 0) : score!.parts.length} instruments <span aria-hidden="true">·</span> Source files stay intact</Description></div><Chip>{museScore ? "MuseScore" : "MusicXML"}</Chip></div>
          <Description>Closing or replacing this score discards its editing choices.</Description>
          {session.stage === "selection" ? <section className="mode-selection" aria-labelledby="mode-selection-heading">
            <div className="mode-intro"><Description>CHOOSE YOUR NEXT STEP</Description><h2 id="mode-selection-heading" className="text-2xl font-semibold sm:text-3xl">What would you like to make?</h2><p className="text-muted">A little space for each way you work with music.</p></div>
            <div className="mode-grid">
              {modes.map(({ kind, title, description, icon }) => <Button key={kind} variant="outline" fullWidth className="mode-tile" data-mode={kind}
                aria-labelledby={`mode-${kind}-title`} aria-describedby={`mode-${kind}-description`}
                isDisabled={busy() || (kind === "practice" && !session.practice)} onPress={() => void changeOperation(kind)}
                ref={(node) => { if (node && focusTarget === kind) { focusTarget = null; node.focus(); } }}>
                <span className="mode-icon" aria-hidden="true"><svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><path d={icon} /></svg></span>
                <span className="mode-copy"><span id={`mode-${kind}-title`} className="mode-title">{title}</span><span id={`mode-${kind}-description`} className="mode-description">{kind === "practice" && !session.practice ? "Requires a MuseScore .mscz or .mscx score." : description}</span></span>
              </Button>)}
            </div>
            {session.practice && <Description>Split, merge, and diff convert visible MuseScore instruments and staves to MusicXML.</Description>}
          </section> : <>
          <div className="mode-configuration-heading flex flex-wrap items-center gap-4">
            <Button variant="ghost" data-action="back-to-modes" isDisabled={busy()} onPress={backToModes}><span aria-hidden="true">←</span> Back to modes</Button>
            <h2 id="mode-config-heading" className="text-xl font-semibold" tabIndex={-1}
              ref={(node) => { if (node && focusTarget === "configuration") { focusTarget = null; node.focus(); } }}>{modes.find(({ kind }) => kind === session.arrangement.kind)!.title}</h2>
          </div>
          {session.arrangement.kind === "practice" && session.practice ? <PracticeView parts={score!.practice_parts ?? []} options={session.practice} config={config} environment={environment} checking={checking}
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
            }} onBrowse={(kind) => void browse(kind)} onDetect={() => void refreshEnvironment()} onExport={() => void exportScore()} /> : session.arrangement.kind === "diff" ? diffView(session) : arrangementView(session)}
          </>}
        </div>}
      </section></main>
      <div className={`status mx-auto max-w-6xl px-5 pb-6 text-sm whitespace-pre-line wrap-anywhere sm:px-7 ${messageError ? "text-danger" : "text-muted"}`} role={messageError ? "alert" : "status"} aria-live="polite">{message}</div>
      {(completedExport || (exporting && session?.arrangement.kind === "practice")) && <PracticeExportModal result={completedExport}
        progress={session?.progress} total={session?.practice?.parts.filter((part) => part.create_track).length ?? 0}
        cancelling={cancelling} onCancel={() => void cancelExport()} onClose={() => { completedExport = null; render(); }} />}
      {pendingPair && <DiffPairModal pair={pendingPair} busy={busy()} closing={releasedPairIds.size > 0} error={pairError} status={message} onChoose={(version) => void chooseDiffFile(version)} onSwap={swapDiffFiles} onCancel={() => void cancelPair()} onCompare={() => void exportDiff()} />}
      {dropping && <div className="drop-overlay pointer-events-none fixed inset-4 z-50 grid place-items-center bg-overlay" aria-hidden="true"><Surface className="p-6"><p>Drop one score to open it or two to compare</p><Description>MusicXML · MuseScore</Description></Surface></div>}
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
    // Active exports own their session until completion; never release it underneath the renderer.
    if (!exporting && !fileBusy) releaseAfterDispose();
    root.unmount();
  };
}
