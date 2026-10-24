import { createRoot } from "react-dom/client";
import { Button, Card, Checkbox, Chip, Description, Label, ListBox, Radio, RadioGroup, Select, Surface } from "@heroui/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  addGroup, fallbackVoice, movePart, newArrangement, newPractice, operation,
  practiceOptions, setFallbackVoice, stemDirection,
  type Arrangement, type Inspection, type Part, type PracticeConfig,
  type PracticeEnvironment, type PracticeOptions, type PracticeProgress, type PracticeResult,
} from "./model.ts";
import { PracticeView } from "./practice-view.tsx";
import { PracticeExportModal } from "./practice-export-modal.tsx";
import { RecentSoundsProvider } from "./sound-picker.tsx";
import { readRecentScores, rememberScore } from "./recent-scores.ts";
import { RecentScoreButton } from "./recent-score-button.tsx";

interface ScoreSession {
  document: Inspection;
  older?: Inspection;
  arrangement: Arrangement;
  practice?: PracticeOptions;
  progress?: PracticeProgress;
  result?: PracticeResult;
}

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
  return <section className="split-preview mb-5" aria-label={`Split preview for ${part.name || part.id} ${part.id}`}><Surface variant="secondary" className="space-y-3 p-4">
    <p><strong>{lanes.length} additional {lanes.length === 1 ? "staff" : "staves"}</strong> · Original staff retained</p>
    {state.mode === "chords" && <p>Chord-tone staffs may also carry compatible voices in measures without that chord tone.</p>}
    <p>A fallback is used only when the staff’s native voices are absent for the whole measure.</p>
    {lanes.map((lane, li) => <Choice key={`${lane.voice}:${lane.tone}`} id={`fallback-${part.id}-${lane.voice}-${lane.tone}`}
      label={`${part.name || part.id} ${part.id} · Additional staff ${li + 1} · Voice ${lane.voice}${state.mode === "chords" ? ` · Chord tone ${lane.tone + 1}` : ""} · Fallback voice`}
      value={fallbackVoice(state, part.id, lane) === null ? "none" : `voice:${fallbackVoice(state, part.id, lane)}`}
      options={[{ key: "none", label: "None (silent)" }, ...voices.filter((voice) => voice !== lane.voice).map((voice) => ({ key: `voice:${voice}`, label: `Voice ${voice}` }))]}
      disabled={busy} onChange={(value) => { setFallbackVoice(state, part.id, lane, value === "none" ? null : value.slice(6)); onChange(); }} />)}
  </Surface></section>;
}

export function mountApp(container: HTMLElement): () => void {
  const root = createRoot(container);
  let current: ScoreSession | null = null;
  let fileBusy = false;
  let exporting = false;
  let cancelling = false;
  let dropping = false;
  let disposed = false;
  let message = "";
  let messageError = false;
  let completedExport: PracticeResult | null = null;
  const config: PracticeConfig = { executable: null, soundfont: null };
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
    if (!disposed || !current) return;
    const session = current;
    current = null;
    void releaseSession(session).catch(console.error);
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
        document: candidate, arrangement,
        practice: candidate.format === "musescore" ? newPractice(candidate.practice_parts ?? []) : undefined,
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
    if (busy()) { notify("Finish the current operation before opening another score.", true); return; }
    if (paths && paths.length !== 1) { notify("Drop one score at a time.", true); return; }
    fileBusy = true;
    render();
    try {
      const path = paths ? paths[0] : await invoke<string | null>("open_file");
      if (path && !disposed) await replaceScore(path);
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

  async function openOlder() {
    if (!current || busy() || disposed) return;
    const session = current;
    let candidate: Inspection | undefined;
    fileBusy = true;
    render();
    try {
      const path = await invoke<string | null>("open_file");
      if (!path || disposed) return;
      candidate = await invoke<Inspection>("inspect_file", { path });
      if (candidate.error) throw new Error(candidate.error);
      if (candidate.format === "musescore") throw new Error("Choose an older MusicXML (.mxml, .musicxml, .xml, .mxl) score.");
      if (disposed || current !== session) return;
      if (session.older) await release(session.older.id);
      session.older = candidate;
      candidate = undefined;
      message = "";
      messageError = false;
    } catch (error) { notify(`Could not open older version: ${String(error)}`, true); }
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
    if (!current || busy() || disposed || (current.practice && checking)) return;
    const session = current;
    try {
      const options = session.practice ? practiceOptions(session.practice, environment?.presets ?? []) : null;
      const request = options ? null : operation(session.arrangement, session.document.parts, session.older?.id);
      if (options && (!environment?.executable || !environment.soundfont || environment.error))
        throw new Error(environment?.error ?? "Configure MuseScore 4.7.x and its MS Basic SoundFont before rendering.");
      exporting = true;
      cancelling = false;
      completedExport = null;
      session.result = undefined;
      session.progress = undefined;
      notify(options ? "Choose a folder for your practice tracks." : session.arrangement.kind === "diff" ? "Choose where to save your marked newer version." : "Choose where to save your arranged score.");
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
          ...(result.warnings.length ? [session.arrangement.kind === "diff" ? "Review warnings:" : "Review warnings — affected passages are marked red:", ...result.warnings] : []),
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
    if (!current?.practice || !exporting || cancelling || disposed) return;
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
        <Card.Header className="section-heading flex-row flex-wrap items-center justify-between gap-4"><div className="min-w-0"><Description>01 / INSTRUMENTS</Description>
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
              <Checkbox.Content><Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                <span className="part-number hidden text-sm text-muted sm:inline" aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
                <Label className="part-name min-w-0"><strong>{part.name || part.id}</strong><Description>{part.instrument_names.join(", ") || "Unnamed instrument"} · {part.id}</Description></Label>
                <span className="voice-count text-sm text-muted">{state.kind === "merge" && assigned.has(part.id) ? "Grouped" : `${part.voice_count} ${part.voice_count === 1 ? "voice" : "voices"}`}</span>
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
        </Card.Content></Card> : <Card className="options-card"><Card.Header className="section-heading flex-row flex-wrap items-center justify-between gap-4"><div><Description>02 / MERGE GROUPS</Description><Card.Title>Arrange the voices.</Card.Title><Card.Description>{state.selected.size} instruments selected</Card.Description></div>
          <Button data-action="add-group" variant="secondary" isDisabled={busy() || state.selected.size < 2} onPress={() => update(({ arrangement }) => {
            const error = addGroup(arrangement, score.parts);
            message = error ?? "Merge group created. Reorder its instruments if needed."; messageError = !!error;
          })}>＋ Create merge group</Button></Card.Header>
          <Card.Content><div className="groups space-y-6">{state.groups.length === 0 && <Description>Select at least two instruments above to create your first group.</Description>}
            {state.groups.map((group, gi) => <section className="merge-group space-y-4" aria-label={`Merge group ${gi + 1}`} key={gi}>
              <div className="section-heading flex flex-wrap items-center justify-between gap-4"><h4 className="font-semibold">Group {gi + 1}</h4><Button variant="secondary" data-action="remove-group" data-group={gi} isDisabled={busy()} onPress={() => update(({ arrangement }) => { arrangement.groups.splice(gi, 1); })}>Remove group</Button></div>
              <ol className="space-y-3">{group.map((id, pi) => <li key={id}><Surface variant="secondary" className="flex flex-wrap items-center gap-4 p-4"><div className="group-part min-w-0 flex-1"><strong>{score.parts.find((part) => part.id === id)?.name || id}</strong><Description>{id} · Voice {pi + 1}</Description></div>
                <Choice id={`stem-${id}`} label={`Voice ${pi + 1} stem · ${id} · Group ${gi + 1}`} value={stemDirection(state, id, pi)}
                  disabled={busy()} options={[{ key: "up", label: "Up" }, { key: "down", label: "Down" }]}
                  onChange={(value) => update(({ arrangement }) => { arrangement.stems.set(id, value as "up" | "down"); })} />
                <span className="reorder flex gap-2"><Button isIconOnly variant="secondary" aria-label={`Move ${id} up in group ${gi + 1}`} data-action="up" data-group={gi} data-index={pi} isDisabled={busy() || pi === 0}
                  onPress={() => update(({ arrangement }) => movePart(arrangement.groups[gi], pi, -1))}>↑</Button>
                  <Button isIconOnly variant="secondary" aria-label={`Move ${id} down in group ${gi + 1}`} data-action="down" data-group={gi} data-index={pi} isDisabled={busy() || pi === group.length - 1}
                    onPress={() => update(({ arrangement }) => movePart(arrangement.groups[gi], pi, 1))}>↓</Button></span>
              </Surface></li>)}</ol><Description>Top to bottom sets voice order in the merged staff.</Description>
            </section>)}
          </div></Card.Content>
        </Card>}
      <footer className="export-bar sticky bottom-4 z-10"><Surface className="flex flex-wrap items-center justify-between gap-4 p-4"><div><strong>Your arrangement, ready to play.</strong><Description>Export a new score. Your source stays intact.</Description></div>
        <Button data-action="export" isDisabled={busy()} onPress={() => void exportScore()}>{exporting ? "Exporting…" : "Export score"}<span aria-hidden="true"> →</span></Button>
      </Surface></footer>
    </>;
  }

  function diffView(session: ScoreSession) {
    return <Card className="diff-card"><Card.Header>
      <Card.Title>See what changed.</Card.Title>
      <Card.Description>Export the newer version with changed notes, lyrics, dynamics, chord symbols, ties, and slurs marked red. Deleted content marks nearby surviving music.</Card.Description>
    </Card.Header><Card.Content className="space-y-5">
      <div><Label>Newer version</Label><p className="wrap-anywhere">{session.document.name}</p><Description>{session.document.path}</Description></div>
      <div><Label>Older version</Label><p className="wrap-anywhere">{session.older?.name ?? "Choose a version to compare against."}</p>{session.older && <Description>{session.older.path}</Description>}</div>
      <Button variant="secondary" data-action="open-older" isDisabled={busy()} onPress={() => void openOlder()}>{session.older ? "Replace older version" : "Choose older version"}</Button>
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
          <div className="drop-hint flex flex-col gap-1"><Description>or drop one score anywhere here</Description><Description>MusicXML · .mxl · MuseScore .mscz / .mscx</Description></div>
        </div> : <div className="score-workspace space-y-6" key={score!.id}>
          <div className="score-heading flex flex-wrap items-center justify-between gap-4"><div className="min-w-0 space-y-3"><Description>YOUR MUSIC, YOUR ARRANGEMENT</Description><h1 className="text-3xl font-semibold wrap-anywhere">{score!.name}</h1><Description>{museScore ? (score!.practice_parts?.length ?? 0) : score!.parts.length} instruments <span aria-hidden="true">·</span> Source files stay intact</Description></div><Chip>{museScore ? "MuseScore" : "MusicXML"}</Chip></div>
          <Description>Closing or replacing this score discards its editing choices.</Description>
          <RadioGroup className="operation" aria-label="Choose an operation" aria-describedby="operation-format-hint" value={session.arrangement.kind} isDisabled={busy()}
            onChange={(kind) => update(({ arrangement }) => {
              if ((kind === "practice") !== museScore) return;
              arrangement.kind = kind as Arrangement["kind"]; arrangement.selected.clear(); message = "";
            })}>
            {(["split", "merge", "diff", "practice"] as const).map((kind) => <Radio key={kind} value={kind} isDisabled={kind === "practice" ? !museScore : museScore}>
              <Radio.Content><Radio.Control><Radio.Indicator /></Radio.Control><Label>{kind === "split" ? "Split instruments" : kind === "merge" ? "Merge instruments" : kind === "diff" ? "Diff versions" : "Create Practice Tracks"}</Label></Radio.Content>
              <Description>{kind === "split" ? "Give every line its own staff" : kind === "merge" ? "Bring parts into a single staff" : kind === "diff" ? "Mark changes in the newer score" : "Your instrument in the foreground"}</Description>
            </Radio>)}
          </RadioGroup>
          <Description id="operation-format-hint">{museScore ? "Split, merge, and diff require a MusicXML score." : "Practice tracks require a MuseScore .mscz or .mscx score."}</Description>
          {session.practice ? <PracticeView parts={score!.practice_parts ?? []} options={session.practice} config={config} environment={environment} checking={checking}
            busy={busy()} rendering={exporting} cancelling={cancelling} progress={session.progress} result={session.result} onOptionsChange={render}
            onConfigChange={(kind, value) => {
              if (busy()) return;
              config[kind] = value.trim() || null; environment = null; ++environmentRequest; checking = false; render();
            }} onBrowse={(kind) => void browse(kind)} onDetect={() => void refreshEnvironment()} onExport={() => void exportScore()} onCancel={() => void cancelExport()} /> : session.arrangement.kind === "diff" ? diffView(session) : arrangementView(session)}
        </div>}
      </section></main>
      <div className={`status mx-auto max-w-6xl px-5 pb-6 text-sm whitespace-pre-line wrap-anywhere sm:px-7 ${messageError ? "text-danger" : "text-muted"}`} role={messageError ? "alert" : "status"} aria-live="polite">{message || (session ? "Your music stays on this device." : "")}</div>
      {completedExport && <PracticeExportModal result={completedExport} onClose={() => { completedExport = null; render(); }} />}
      {dropping && <div className="drop-overlay pointer-events-none fixed inset-4 z-50 grid place-items-center bg-overlay" aria-hidden="true"><Surface className="p-6"><p>Drop one score to open it</p><Description>MusicXML · MuseScore</Description></Surface></div>}
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
