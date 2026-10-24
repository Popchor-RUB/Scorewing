import { createRoot } from "react-dom/client";
import { Button, Card, Checkbox, Label, ListBox, Radio, RadioGroup, Select } from "@heroui/react";
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
import { RecentSoundsProvider } from "./sound-picker.tsx";

interface ScoreSession {
  document: Inspection;
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
  return <Select id={id} className="score-select" isDisabled={disabled} value={value}
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
  return <section className="split-preview" aria-label={`Split preview for ${part.name || part.id} ${part.id}`}>
    <p><strong>{lanes.length} additional {lanes.length === 1 ? "staff" : "staves"}</strong> · Original staff retained</p>
    {state.mode === "chords" && <p>Chord-tone staffs may also carry compatible voices in measures without that chord tone.</p>}
    <p>A fallback is used only when the staff’s native voices are absent for the whole measure.</p>
    {lanes.map((lane, li) => <Choice key={`${lane.voice}:${lane.tone}`} id={`fallback-${part.id}-${lane.voice}-${lane.tone}`}
      label={`${part.name || part.id} ${part.id} · Additional staff ${li + 1} · Voice ${lane.voice}${state.mode === "chords" ? ` · Chord tone ${lane.tone + 1}` : ""} · Fallback voice`}
      value={fallbackVoice(state, part.id, lane) === null ? "none" : `voice:${fallbackVoice(state, part.id, lane)}`}
      options={[{ key: "none", label: "None (silent)" }, ...voices.filter((voice) => voice !== lane.voice).map((voice) => ({ key: `voice:${voice}`, label: `Voice ${voice}` }))]}
      disabled={busy} onChange={(value) => { setFallbackVoice(state, part.id, lane, value === "none" ? null : value.slice(6)); onChange(); }} />)}
  </section>;
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
  const config: PracticeConfig = { executable: null, soundfont: null };
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

  function releaseAfterDispose() {
    if (!disposed || !current) return;
    const id = current.document.id;
    current = null;
    void release(id).catch(console.error);
  }

  async function replaceScore(path: string) {
    let candidate: Inspection | undefined;
    try {
      candidate = await invoke<Inspection>("inspect_file", { path });
      if (candidate.error) throw new Error(candidate.error);
      if (disposed) return;
      if (current) {
        await release(current.document.id);
        current = null;
      }
      if (disposed) return;
      const arrangement = newArrangement();
      if (candidate.format === "musescore") arrangement.kind = "practice";
      current = {
        document: candidate, arrangement,
        practice: candidate.format === "musescore" ? newPractice(candidate.practice_parts ?? []) : undefined,
      };
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
      await release(current.document.id);
      current = null;
      message = "";
      messageError = false;
      window.scrollTo(0, 0);
    } catch (error) { notify(`Could not close score: ${String(error)}`, true); }
    finally { fileBusy = false; if (disposed) releaseAfterDispose(); render(); }
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
      const request = options ? null : operation(session.arrangement, session.document.parts);
      if (options && (!environment?.executable || !environment.soundfont || environment.error))
        throw new Error(environment?.error ?? "Configure MuseScore 4.7.x and its MS Basic SoundFont before rendering.");
      exporting = true;
      cancelling = false;
      session.result = undefined;
      session.progress = undefined;
      notify(options ? "Choose a folder for your practice tracks." : "Choose where to save your arranged score.");
      if (options) {
        const result = await invoke<PracticeResult | null>("export_practice_tracks", {
          id: session.document.id, options, config: { ...config },
        });
        if (!disposed && current === session) {
          session.result = result ?? undefined;
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
        <Card.Header className="section-heading"><div><span className="eyebrow">01 / INSTRUMENTS</span>
          <Card.Title>{state.kind === "split" ? "Make room for every voice." : "Bring your parts together."}</Card.Title>
          <Card.Description>{state.kind === "split" ? "Choose instruments to split. Originals stay in place; new staves appear below them." : "Choose two or more instruments for a group. Other parts stay as they are."}</Card.Description>
        </div><Button variant="ghost" data-action="select-all" isDisabled={busy() || !available.length} onPress={() => update(({ arrangement }) => {
          arrangement.selected = allSelected ? new Set() : new Set(available.map((part) => part.id));
        })}>{allSelected ? "Clear selection" : "Select all"}</Button></Card.Header>
        <Card.Content><div className="instruments" role="group" aria-label="Instruments">
          {score.parts.map((part, index) => <div className="instrument-entry" key={part.id}>
            <Checkbox className="instrument" data-part={part.id} isSelected={state.selected.has(part.id)}
              isDisabled={busy() || (state.kind === "merge" && assigned.has(part.id))}
              onChange={(selected) => update(({ arrangement }) => { if (selected) arrangement.selected.add(part.id); else arrangement.selected.delete(part.id); })}>
              <Checkbox.Content><Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                <span className="part-number" aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
                <Label className="part-name"><strong>{part.name || part.id}</strong><small>{part.instrument_names.join(", ") || "Unnamed instrument"} · {part.id}</small></Label>
                <span className="voice-count">{state.kind === "merge" && assigned.has(part.id) ? "Grouped" : `${part.voice_count} ${part.voice_count === 1 ? "voice" : "voices"}`}</span>
              </Checkbox.Content>
            </Checkbox>
            {state.kind === "split" && state.selected.has(part.id) && <SplitPreview part={part} state={state} busy={busy()} onChange={render} />}
          </div>)}
        </div></Card.Content>
      </Card>
      {state.kind === "split" ? <Card className="options-card"><Card.Header><span className="eyebrow">02 / SPLIT SETTINGS</span><Card.Title>A staff for each line.</Card.Title></Card.Header>
        <Card.Content className="settings-grid"><div><Choice id="split-mode" label="Split by" value={state.mode} disabled={busy()}
          options={[{ key: "chords", label: "Chord tones" }, { key: "voices", label: "Explicit voices" }]}
          onChange={(mode) => update(({ arrangement }) => { arrangement.mode = mode as Arrangement["mode"]; })} />
          <p className="hint">{state.mode === "chords" ? "Separate simultaneous notes by pitch into individual staves." : "Separate the voices already written into the score."}</p></div>
          <div><Checkbox id="insert-fallback-slurs" isSelected={state.insertFallbackSlurs} isDisabled={busy()}
            onChange={(selected) => update(({ arrangement }) => { arrangement.insertFallbackSlurs = selected; })}>
            <Checkbox.Content><Checkbox.Control><Checkbox.Indicator /></Checkbox.Control><Label>Automatically insert slurs at fallback voice transitions</Label></Checkbox.Content>
          </Checkbox><p className="hint">Matching pitches continue existing ties. When pitches differ, this option uses a slur and marks its endpoint notes red for review. When disabled, the fallback note becomes a rest of the same duration.</p></div>
        </Card.Content></Card> : <Card className="options-card"><Card.Header className="section-heading"><div><span className="eyebrow">02 / MERGE GROUPS</span><Card.Title>Arrange the voices.</Card.Title><Card.Description>{state.selected.size} instruments selected</Card.Description></div>
          <Button data-action="add-group" variant="secondary" isDisabled={busy() || state.selected.size < 2} onPress={() => update(({ arrangement }) => {
            const error = addGroup(arrangement, score.parts);
            message = error ?? "Merge group created. Reorder its instruments if needed."; messageError = !!error;
          })}>＋ Create merge group</Button></Card.Header>
          <Card.Content><div className="groups">{state.groups.length === 0 && <p className="hint">Select at least two instruments above to create your first group.</p>}
            {state.groups.map((group, gi) => <section className="merge-group" aria-label={`Merge group ${gi + 1}`} key={gi}>
              <div className="section-heading"><h4>Group {gi + 1}</h4><Button variant="ghost" data-action="remove-group" data-group={gi} isDisabled={busy()} onPress={() => update(({ arrangement }) => { arrangement.groups.splice(gi, 1); })}>Remove group</Button></div>
              <ol>{group.map((id, pi) => <li key={id}><span className="group-part"><strong>{score.parts.find((part) => part.id === id)?.name || id}</strong><small>{id} · Voice {pi + 1}</small></span>
                <Choice id={`stem-${id}`} label={`Voice ${pi + 1} stem · ${id} · Group ${gi + 1}`} value={stemDirection(state, id, pi)}
                  disabled={busy()} options={[{ key: "up", label: "Up" }, { key: "down", label: "Down" }]}
                  onChange={(value) => update(({ arrangement }) => { arrangement.stems.set(id, value as "up" | "down"); })} />
                <span className="reorder"><Button isIconOnly variant="tertiary" aria-label={`Move ${id} up in group ${gi + 1}`} data-action="up" data-group={gi} data-index={pi} isDisabled={busy() || pi === 0}
                  onPress={() => update(({ arrangement }) => movePart(arrangement.groups[gi], pi, -1))}>↑</Button>
                  <Button isIconOnly variant="tertiary" aria-label={`Move ${id} down in group ${gi + 1}`} data-action="down" data-group={gi} data-index={pi} isDisabled={busy() || pi === group.length - 1}
                    onPress={() => update(({ arrangement }) => movePart(arrangement.groups[gi], pi, 1))}>↓</Button></span>
              </li>)}</ol><p className="hint">Top to bottom sets voice order in the merged staff.</p>
            </section>)}
          </div></Card.Content>
        </Card>}
      <footer className="export-bar"><div><strong>Your arrangement, ready to play.</strong><p>Export a new score. Your source stays intact.</p></div>
        <Button data-action="export" isDisabled={busy()} onPress={() => void exportScore()}>{exporting ? "Exporting…" : "Export score"}<span aria-hidden="true"> →</span></Button>
      </footer>
    </>;
  }

  function render() {
    if (disposed) return;
    const session = current;
    const score = session?.document;
    const museScore = score?.format === "musescore";
    root.render(<RecentSoundsProvider><div className="app-shell">
      <header className="app-header"><div className="brand"><span className="brand-icon" aria-hidden="true">♫</span><div><strong>Voiceweaver</strong><span className="tagline">A little room to rearrange.</span></div></div>
        <div className="file-actions">{session && <Button variant="ghost" data-action="close" isDisabled={busy()} onPress={() => void closeScore()}>Close score</Button>}
          <Button data-action="open" variant={session ? "secondary" : "primary"} isDisabled={busy()} onPress={() => void openScore()}>{fileBusy ? "Opening…" : session ? "Open another score" : "Open score"}<span aria-hidden="true"> ↗</span></Button>
        </div>
      </header>
      <main className="workspace"><section className="editor" aria-label="Score arrangement">
        {!session ? <div className="empty"><div className="score-mark" aria-hidden="true">♫</div><span className="eyebrow">MAKE SPACE FOR YOUR MUSIC</span><h1>Every voice.<br />A little clearer.</h1>
          <p>Split a staff, bring parts together, or make a practice track that lets your instrument shine.</p>
          <Button data-action="open" isDisabled={busy()} onPress={() => void openScore()}>{fileBusy ? "Reading your score…" : "Choose a score"}<span aria-hidden="true"> →</span></Button>
          <div className="drop-hint"><span>or drop one score anywhere here</span><small>MusicXML · .mxl · MuseScore .mscz / .mscx</small></div>
        </div> : <div className="score-workspace" key={score!.id}>
          <div className="score-heading"><div><span className="eyebrow">YOUR MUSIC, YOUR ARRANGEMENT</span><h1>{score!.name}</h1><p>{museScore ? (score!.practice_parts?.length ?? 0) : score!.parts.length} instruments <span aria-hidden="true">·</span> Source files stay intact</p></div><span className="score-badge">{museScore ? "MuseScore" : "MusicXML"}</span></div>
          <p className="close-hint">Closing or replacing this score discards its editing choices.</p>
          <RadioGroup className="operation" aria-label="Choose an operation" aria-describedby="operation-format-hint" value={session.arrangement.kind} isDisabled={busy()}
            onChange={(kind) => update(({ arrangement }) => {
              if ((kind === "practice") !== museScore) return;
              arrangement.kind = kind as Arrangement["kind"]; arrangement.selected.clear(); message = "";
            })}>
            {(["split", "merge", "practice"] as const).map((kind) => <Radio key={kind} value={kind} className="operation-choice" isDisabled={kind === "practice" ? !museScore : museScore}>
              <Radio.Content><Radio.Control><Radio.Indicator /></Radio.Control><Label><strong>{kind === "split" ? "Split instruments" : kind === "merge" ? "Merge instruments" : "Create Practice Tracks"}</strong><small>{kind === "split" ? "Give every line its own staff" : kind === "merge" ? "Bring parts into a single staff" : "Your instrument in the foreground"}</small></Label></Radio.Content>
            </Radio>)}
          </RadioGroup>
          <p className="hint format-hint" id="operation-format-hint">{museScore ? "Split and merge require a MusicXML score." : "Practice tracks require a MuseScore .mscz or .mscx score."}</p>
          {session.practice ? <PracticeView parts={score!.practice_parts ?? []} options={session.practice} config={config} environment={environment} checking={checking}
            busy={busy()} rendering={exporting} cancelling={cancelling} progress={session.progress} result={session.result} onOptionsChange={render}
            onConfigChange={(kind, value) => {
              if (busy()) return;
              config[kind] = value.trim() || null; environment = null; ++environmentRequest; checking = false; render();
            }} onBrowse={(kind) => void browse(kind)} onDetect={() => void refreshEnvironment()} onExport={() => void exportScore()} onCancel={() => void cancelExport()} /> : arrangementView(session)}
        </div>}
      </section></main>
      <div className={`status ${messageError ? "error-text" : ""}`} role={messageError ? "alert" : "status"} aria-live="polite">{message || "Your music stays on this device."}</div>
      {dropping && <div className="drop-overlay" aria-hidden="true"><div>Drop one score to open it<span>MusicXML · MuseScore</span></div></div>}
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
