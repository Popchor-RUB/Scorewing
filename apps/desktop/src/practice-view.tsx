import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "./components/ui/button.tsx";
import { Checkbox } from "./components/ui/checkbox.tsx";
import { Input } from "./components/ui/input.tsx";
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogTitle } from "./components/ui/dialog.tsx";
import type {
  PracticeConfig, PracticeDefaults, PracticeEnvironment, PracticeOptions, PracticePart,
  PracticeResult, PracticeSection, PresetId,
} from "./model.ts";
import { SoundPicker } from "./sound-picker.tsx";

export interface PracticeViewProps {
  parts: PracticePart[];
  sections: PracticeSection[];
  options: PracticeOptions;
  defaults: PracticeDefaults;
  onSetDefault: (mode: "foreground_sound" | "background_sound", sound: PresetId) => void;
  onSetLevelDefault: (field: "background_reduction_db" | "metronome_db") => void;
  config: PracticeConfig;
  environment: PracticeEnvironment | null;
  checking: boolean;
  busy: boolean;
  rendering: boolean;
  result?: PracticeResult;
  onOptionsChange: () => void;
  onConfigChange: (kind: keyof PracticeConfig, value: string) => void;
  onBrowse: (kind: keyof PracticeConfig) => void;
  onDetect: () => void;
  onExport: () => void;
}

function RendererPath({ kind, value, placeholder, disabled, onChange, onBrowse }: {
  kind: keyof PracticeConfig;
  value: string | null;
  placeholder: string;
  disabled: boolean;
  onChange: PracticeViewProps["onConfigChange"];
  onBrowse: PracticeViewProps["onBrowse"];
}) {
  const [draft, setDraft] = useState(value ?? "");
  const label = kind === "executable" ? "MuseScore executable" : "MS Basic SoundFont";
  useEffect(() => { setDraft(value ?? ""); }, [value]);
  return <div className="path-row flex items-end gap-3">
    <label className="practice-path min-w-0 flex-1"><span className="muted">{label}</span>
      <Input value={draft} disabled={disabled} placeholder={placeholder} onChange={(event) => setDraft(event.target.value)}
        onBlur={() => { if (draft !== (value ?? "")) onChange(kind, draft); }} />
    </label>
    <Button variant="outline" disabled={disabled} aria-label={`Browse ${label}`} data-action="practice-browse" data-kind={kind} onClick={() => onBrowse(kind)}>Browse</Button>
  </div>;
}

function PracticeResults({ result }: { result: PracticeResult }) {
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState("");
  async function openFolder() {
    if (opening) return;
    setOpening(true); setError("");
    try { await invoke("open_practice_export_folder", { directory: result.directory }); }
    catch (error) { setError(`Could not open the folder: ${String(error)}`); }
    finally { setOpening(false); }
  }
  return <section className="practice-results" aria-label="Practice track results">
    <h3>{result.cancelled ? "Rendering cancelled" : result.error ? "Rendering stopped" : "Practice tracks created"}</h3>
    {result.error && <p className="text-danger" role="alert">{result.error}</p>}
    <p>{result.completed.length} completed {result.completed.length === 1 ? "track was" : "tracks were"} saved and {result.completed.length === 1 ? "is" : "are"} available.</p>
    <p className="muted wrap-anywhere">{result.directory}</p>
    <ul className="practice-result-files">{result.completed.map((track) => <li className="wrap-anywhere" key={track.key}><strong>{track.name}</strong><p className="muted">{track.path}</p></li>)}</ul>
    <Button variant="outline" disabled={opening} data-action="practice-partial-open-folder" onClick={() => void openFolder()}>{opening ? "Opening…" : "Open folder"}</Button>
    {error && <p role="alert" className="text-danger">{error}</p>}
  </section>;
}

export function PracticeView({
  parts, sections, options, defaults, onSetDefault, onSetLevelDefault, config, environment, checking, busy, rendering,
  result, onOptionsChange, onConfigChange, onBrowse, onDetect, onExport,
}: PracticeViewProps) {
  const [compact, setCompact] = useState(() => window.matchMedia("(max-width: 900px)").matches);
  const [mixOpen, setMixOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const mixButton = useRef<HTMLButtonElement>(null);
  const rendererButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const media = window.matchMedia("(max-width: 900px)");
    const update = () => setCompact(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  const disabled = busy || checking;
  const rendererReady = !!(environment?.executable && environment.soundfont && environment.version && !environment.error);
  const instrumentsDisabled = disabled || !rendererReady;
  const trackCount = options.parts.filter((part) => part.create_track).length;
  const rangeCount = Number(options.include_whole_song) + options.section_keys.length;
  const mix = <div className="practice-mix-controls flex flex-col gap-5">
    <div><h3>Practice mix</h3><p className="flex justify-between muted"><span>Foreground</span><span>0 dB</span></p></div>
    <div className="practice-levels flex flex-col gap-5">
      <div>
        <label id="background-reduction-label" htmlFor="background-reduction" className="muted">Background reduction (dB)</label>
        <Input id="background-reduction" aria-labelledby="background-reduction-label" type="number" min={0} max={60} step={1} disabled={busy}
          value={Number.isFinite(options.background_reduction_db) ? options.background_reduction_db : ""}
          onChange={(event) => { options.background_reduction_db = event.target.valueAsNumber; onOptionsChange(); }} />
        {Number.isFinite(options.background_reduction_db) && options.background_reduction_db !== (defaults.background_reduction_db ?? 6) &&
          <Button variant="ghost" size="sm" disabled={busy} aria-label="Set default background reduction" onClick={() => onSetLevelDefault("background_reduction_db")}>Set Default</Button>}
      </div>
      <label className="checkbox-row"><Checkbox id="practice-metronome" checked={options.metronome_enabled} disabled={busy}
        onCheckedChange={(value) => { options.metronome_enabled = value; onOptionsChange(); }} />Enable metronome</label>
      <div>
        <label id="metronome-level-label" htmlFor="metronome-level" className="muted">Metronome level (dB)</label>
        <Input id="metronome-level" aria-labelledby="metronome-level-label" type="number" min={-60} max={0} step={1} disabled={busy || !options.metronome_enabled}
          value={Number.isFinite(options.metronome_db) ? options.metronome_db : ""}
          onChange={(event) => { options.metronome_db = event.target.valueAsNumber; onOptionsChange(); }} />
        {Number.isFinite(options.metronome_db) && options.metronome_db !== (defaults.metronome_db ?? -6) &&
          <Button variant="ghost" size="sm" disabled={busy || !options.metronome_enabled} aria-label="Set default metronome level" onClick={() => onSetLevelDefault("metronome_db")}>Set Default</Button>}
      </div>
    </div>
    <p className="muted">Foreground stays at 0 dB. Background instruments are reduced by this amount. Uses MuseScore’s built-in metronome. No count-in is added.</p>
    <section className="practice-environment" aria-label="MuseScore renderer">
      <h3>MuseScore settings</h3>
      {checking ? <p role="status">Checking MuseScore…</p> : rendererReady
        ? <p className="text-success" role="status">Found and using MuseScore Version {environment!.version}</p>
        : <p className="text-danger" role="alert"><span aria-hidden="true">⚠ </span>MuseScore setup needs attention. Open settings.</p>}
      <Button ref={rendererButton} variant="outline" aria-label="MuseScore settings" data-action="practice-settings" disabled={busy}
        onClick={() => { setMixOpen(false); setSettingsOpen(true); }}>Configure renderer and sounds</Button>
      <p className="muted">Keep MuseScore closed while rendering.</p>
    </section>
  </div>;

  return <Dialog open={settingsOpen} onOpenChange={setSettingsOpen}>
    <div className="practice-view workspace-view">
      <div className="workspace-layout practice-layout">
        <section className="workspace-pane ranges-pane" aria-label="Practice ranges">
          <header className="pane-header"><h2>Ranges</h2>
            <div className="pane-selection-actions">
              <Button variant="ghost" size="sm" disabled={disabled} data-action="practice-select-all"
                onClick={() => { options.include_whole_song = true; options.section_keys = sections.map((section) => section.key); onOptionsChange(); }}>Select all</Button>
              <span aria-hidden="true">·</span>
              <Button variant="ghost" size="sm" disabled={disabled} data-action="practice-clear-all"
                onClick={() => { options.include_whole_song = false; options.section_keys = []; onOptionsChange(); }}>Clear</Button>
            </div>
          </header>
          <div className="pane-body flex flex-col gap-5">
            <label className="checkbox-row" data-action="practice-whole-song"><Checkbox checked={options.include_whole_song} disabled={disabled}
              onCheckedChange={(selected) => { options.include_whole_song = selected; onOptionsChange(); }} />Whole song</label>
            <h3>Rehearsal sections</h3>
            {sections.map((section) => <label className="checkbox-row" key={section.key} data-section-key={section.key}>
              <Checkbox checked={options.section_keys.includes(section.key)} disabled={disabled} onCheckedChange={(selected) => {
                options.section_keys = selected ? [...options.section_keys, section.key] : options.section_keys.filter((key) => key !== section.key); onOptionsChange();
              }} /><span>{section.name}<small className="muted block">Measures {section.start_measure}–{section.end_measure}</small></span>
            </label>)}
            <p className="muted">{sections.length
              ? "Sections start on the first playback arrival at their mark, follow repeats and jumps, and include up to two played measures after leaving the displayed range. Each range has its own output folder."
              : "No rehearsal marks found. Whole-song export is available."}</p>
            {!options.include_whole_song && !options.section_keys.length && <p className="text-danger" role="alert">Choose the whole song or at least one rehearsal section.</p>}
          </div>
        </section>

        <section className="workspace-pane instruments-pane" aria-label="Practice instruments" aria-disabled={instrumentsDisabled}>
          <header className="pane-header practice-instruments-header"><h2>Instruments</h2><span className="muted">{trackCount} tracks enabled</span>
            {compact && <Button ref={mixButton} className="practice-compact-settings" variant="outline" disabled={busy} onClick={() => setMixOpen(true)}>Mix &amp; renderer</Button>}
            <p className="muted practice-instruments-explanation">Check Create practice track for one MP3 per selected range.<br />Background instruments accompany the other tracks.</p>
          </header>
          <div className="pane-body practice-parts">
            {[true, false].map((visible) => {
              const group = parts.map((part, index) => ({ part, index })).filter(({ part }) => part.visible === visible);
              const groupId = `practice-${visible ? "visible" : "hidden"}-instruments`;
              return group.length > 0 && <section className="practice-visibility-group" key={groupId} aria-labelledby={groupId}>
                <h3 className="instrument-visibility-heading" id={groupId}>{visible ? "Visible instruments" : "Hidden instruments"}</h3>
                {group.map(({ part, index }) => {
                  const setting = options.parts[index];
                  const label = part.name || "Unnamed instrument";
                  return <section className="practice-part" key={part.key} role="group" aria-labelledby={`practice-part-${index}`}>
                    <h4 className="practice-part-heading" id={`practice-part-${index}`}>{label}</h4>
                    <div className="practice-sound-row">
                      <label className="checkbox-row practice-track-choice" data-practice-index={index} data-practice-field="create_track">
                        <Checkbox checked={setting.create_track} disabled={instrumentsDisabled} onCheckedChange={(value) => { setting.create_track = value; onOptionsChange(); }} />
                        Create practice track<span className="sr-only"> · {label}</span>
                      </label>
                      <SoundPicker id={`foreground_sound-${index}`} label="Foreground Instrument"
                          aria-label={`${label}, instrument ${index + 1}`}
                          presets={environment?.presets ?? []} value={setting.foreground_sound} isDisabled={instrumentsDisabled || !setting.create_track}
                          defaultValue={defaults.foreground_sound} onSetDefault={(sound) => onSetDefault("foreground_sound", sound)}
                          onChange={(value) => { setting.foreground_sound = value; onOptionsChange(); }} />
                    </div>
                    <div className="practice-sound-row">
                      <label className="checkbox-row" data-practice-index={index} data-practice-field="include_in_background"><Checkbox checked={setting.include_in_background} disabled={instrumentsDisabled}
                        onCheckedChange={(value) => { setting.include_in_background = value; onOptionsChange(); }} />Include in background<span className="sr-only"> · {label}</span></label>
                      <SoundPicker id={`background_sound-${index}`} label="Background Instrument"
                          aria-label={`${label}, instrument ${index + 1}`}
                          presets={environment?.presets ?? []} value={setting.background_sound} isDisabled={instrumentsDisabled || !setting.include_in_background}
                          defaultValue={defaults.background_sound} onSetDefault={(sound) => onSetDefault("background_sound", sound)}
                          onChange={(value) => { setting.background_sound = value; onOptionsChange(); }} />
                    </div>
                  </section>;
                })}
              </section>;
            })}
            {result && <PracticeResults key={result.directory} result={result} />}
          </div>
        </section>

        {!compact && <aside className="workspace-pane practice-mix" aria-label="Practice mix"><div className="pane-body">{mix}</div></aside>}
      </div>
      <footer className="workspace-footer"><p className="muted">{trackCount} instruments · {rangeCount} ranges<span className="sr-only"> · 192 kbps MP3 · Source score stays intact</span></p>
        <Button disabled={instrumentsDisabled} data-action="practice-export" onClick={onExport}>{rendering ? "Rendering…" : "Create Practice Tracks"}</Button>
      </footer>
    </div>
    {compact && <Dialog open={mixOpen} onOpenChange={setMixOpen}><DialogContent className="practice-mix-sheet">
      <DialogTitle>Mix &amp; renderer</DialogTitle><DialogDescription>Practice track levels and MuseScore setup.</DialogDescription>
      {mix}<div className="dialog-footer"><DialogClose render={<Button variant="outline" />}>Done</DialogClose></div>
    </DialogContent></Dialog>}
    <DialogContent className="renderer-settings" finalFocus={compact ? mixButton : rendererButton}>
      <div className="dialog-header"><DialogTitle>MuseScore settings</DialogTitle>
        <DialogClose render={<Button variant="ghost" size="icon" />} aria-label="Close MuseScore settings">×</DialogClose></div>
      <DialogDescription>MuseScore 4.7.x and its matching MS Basic SoundFont are required for MP3 export.</DialogDescription>
      <div className="dialog-body flex flex-col gap-5">
        {(["executable", "soundfont"] as const).map((kind) => <RendererPath key={kind} kind={kind} value={config[kind]}
          placeholder={environment?.[kind] ?? "Detect automatically"} disabled={disabled} onChange={onConfigChange} onBrowse={onBrowse} />)}
        <Button className="self-start" variant="outline" disabled={disabled} data-action="practice-detect" onClick={onDetect}>{checking ? "Checking renderer…" : "Check renderer and sounds"}</Button>
        <p className="muted">Check renderer and sounds after changing a path.</p>
        {environment?.version && <p className="text-success">Detected MuseScore {environment.version}</p>}
        {environment?.error && <p className="text-danger" role="alert">{environment.error}</p>}
        <p className="muted">Keep MuseScore closed during rendering. Scorewing temporarily controls the built-in metronome preference and restores it when rendering ends.</p>
      </div>
      <div className="dialog-footer"><DialogClose render={<Button />}>Done</DialogClose></div>
    </DialogContent>
  </Dialog>;
}
