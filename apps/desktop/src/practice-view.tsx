import { Button, Card, Checkbox, Description, Input, Label, Modal, NumberField, Surface, TextField } from "@heroui/react";
import { useEffect, useState } from "react";
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
  return <div className="path-row mb-4 flex flex-wrap items-end gap-3">
    <TextField className="practice-path min-w-0 flex-1" value={draft} onChange={setDraft} isDisabled={disabled}
      onBlur={() => { if (draft !== (value ?? "")) onChange(kind, draft); }}>
      <Label>{label}</Label>
      <Input variant="secondary" placeholder={placeholder} />
    </TextField>
    <Button variant="secondary" isDisabled={disabled} aria-label={`Browse ${label}`} data-action="practice-browse" data-kind={kind} onPress={() => onBrowse(kind)}>Browse</Button>
  </div>;
}

export function PracticeView({
  parts, sections, options, defaults, onSetDefault, onSetLevelDefault, config, environment, checking, busy, rendering,
  result, onOptionsChange, onConfigChange, onBrowse, onDetect, onExport,
}: PracticeViewProps) {
  const disabled = busy || checking;
  const rendererReady = !!(environment?.executable && environment.soundfont && environment.version && !environment.error);
  const instrumentsDisabled = disabled || !rendererReady;
  return <div className="practice-view grid gap-6">
    <div className="practice-environment flex items-center gap-3" aria-label="MuseScore renderer">
      {checking ? <p role="status">Checking MuseScore…</p> : rendererReady
        ? <p role="status">Found and using MuseScore Version {environment!.version}</p>
        : <p className="text-danger" role="alert"><span aria-hidden="true">⚠ </span>MuseScore setup needs attention. Open settings.</p>}
      <Modal>
        <Button isIconOnly size="sm" variant="secondary" aria-label="MuseScore settings" data-action="practice-settings" isDisabled={busy}>
          <svg aria-hidden="true" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
            <path d="m9.5 3-.6 2.4-1.8 1-2.4-.7-2.5 4.4L4 11.8v2l-1.8 1.7 2.5 4.3 2.4-.7 1.8 1 .6 2.4h5l.6-2.4 1.8-1 2.4.7 2.5-4.3-1.8-1.7v-2l1.8-1.7-2.5-4.4-2.4.7-1.8-1-.6-2.4Z" transform="translate(0 -1)" />
            <circle cx="12" cy="12" r="3" />
          </svg>
        </Button>
        <Modal.Backdrop>
          <Modal.Container size="lg" scroll="inside">
            <Modal.Dialog>
              <Modal.CloseTrigger aria-label="Close MuseScore settings" />
              <Modal.Header>
                <Modal.Heading>MuseScore settings</Modal.Heading>
                <Description>MuseScore 4.7.x and its matching MS Basic SoundFont are required for MP3 export.</Description>
              </Modal.Header>
              <Modal.Body className="flex flex-col gap-3">
                {(["executable", "soundfont"] as const).map((kind) => <RendererPath key={kind}
                  kind={kind} value={config[kind]} placeholder={environment?.[kind] ?? "Detect automatically"}
                  disabled={disabled} onChange={onConfigChange} onBrowse={onBrowse} />)}
                <Button className="self-start" variant="secondary" isDisabled={disabled} data-action="practice-detect" onPress={onDetect}>
                  {checking ? "Checking renderer…" : "Check renderer and sounds"}
                </Button>
                <Description>Check renderer and sounds after changing a path.</Description>
                {environment?.version && <Description>Detected MuseScore {environment.version}</Description>}
                {environment?.error && <p className="text-danger" role="alert">{environment.error}</p>}
                <Description>Keep MuseScore closed during rendering. Scorewing temporarily controls the built-in metronome preference and restores it when rendering ends.</Description>
              </Modal.Body>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
    </div>

    <Card aria-label="Practice ranges">
      <Card.Header><Card.Title>Whole song and rehearsal sections</Card.Title></Card.Header>
      <Card.Content className="flex flex-col gap-3">
        <Checkbox variant="secondary" isSelected={options.include_whole_song} isDisabled={disabled}
          data-action="practice-whole-song" onChange={(selected) => {
            options.include_whole_song = selected; onOptionsChange();
          }}>
          <Checkbox.Content><Checkbox.Control><Checkbox.Indicator /></Checkbox.Control><Label>Whole song</Label></Checkbox.Content>
        </Checkbox>
        {sections.map((section) => <Checkbox key={section.key} variant="secondary"
          isSelected={options.section_keys.includes(section.key)} isDisabled={disabled}
          data-section-key={section.key} onChange={(selected) => {
            options.section_keys = selected ? [...options.section_keys, section.key] : options.section_keys.filter((key) => key !== section.key);
            onOptionsChange();
          }}>
          <Checkbox.Content><Checkbox.Control><Checkbox.Indicator /></Checkbox.Control><Label>{section.name}</Label></Checkbox.Content>
          <Description>Measures {section.start_measure}–{section.end_measure}</Description>
        </Checkbox>)}
        <Description>{sections.length
          ? "Sections start on the first playback arrival at their mark, follow repeats and jumps, and include up to two played measures after leaving the displayed range. Each range has its own output folder."
          : "No rehearsal marks found. Whole-song export is available."}</Description>
        {!options.include_whole_song && !options.section_keys.length &&
          <p className="text-danger" role="alert">Choose the whole song or at least one rehearsal section.</p>}
      </Card.Content>
    </Card>

    <section aria-label="Practice instruments" aria-disabled={instrumentsDisabled} className={instrumentsDisabled ? "opacity-50" : undefined}>
      <div className="section-heading mb-4 space-y-2">
        <h2 className="text-xl font-semibold">Practice instruments</h2>
        <Description>The featured instrument uses its foreground sound. Other selected background instruments play more quietly.</Description>
      </div>
      <div className="practice-parts grid gap-4">
        {parts.map((part, index) => {
          const setting = options.parts[index];
          const label = part.name || "Unnamed instrument";
          return <Card className="practice-part" key={part.key} role="group" aria-labelledby={`practice-part-${index}`}>
            <Card.Header>
              <Card.Title id={`practice-part-${index}`}>{label}</Card.Title>
              {!part.visible && <Card.Description>Hidden in score</Card.Description>}
            </Card.Header>
            <Card.Content className="grid gap-5">
              <div className="practice-membership grid gap-3">
                <Checkbox variant="secondary" isSelected={setting.create_track} isDisabled={instrumentsDisabled}
                  data-practice-index={index} data-practice-field="create_track"
                  onChange={(value) => { setting.create_track = value; onOptionsChange(); }}>
                  <Checkbox.Content>
                    <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                    <Label>Create practice track<span className="sr-only"> · {label}</span></Label>
                  </Checkbox.Content>
                </Checkbox>
                {setting.create_track && <SoundPicker id={`foreground_sound-${index}`}
                  label="Foreground Instrument"
                  presets={environment?.presets ?? []} value={setting.foreground_sound} isDisabled={instrumentsDisabled}
                  defaultValue={defaults.foreground_sound} onSetDefault={(sound) => onSetDefault("foreground_sound", sound)}
                  onChange={(value) => { setting.foreground_sound = value; onOptionsChange(); }} />}
              </div>
              <div className="practice-membership grid gap-3">
                <Checkbox variant="secondary" isSelected={setting.include_in_background} isDisabled={instrumentsDisabled}
                  data-practice-index={index} data-practice-field="include_in_background"
                  onChange={(value) => { setting.include_in_background = value; onOptionsChange(); }}>
                  <Checkbox.Content>
                    <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                    <Label>Include in background<span className="sr-only"> · {label}</span></Label>
                  </Checkbox.Content>
                </Checkbox>
                {setting.include_in_background && <SoundPicker id={`background_sound-${index}`}
                  label="Background Instrument"
                  presets={environment?.presets ?? []} value={setting.background_sound} isDisabled={instrumentsDisabled}
                  defaultValue={defaults.background_sound} onSetDefault={(sound) => onSetDefault("background_sound", sound)}
                  onChange={(value) => { setting.background_sound = value; onOptionsChange(); }} />}
              </div>
            </Card.Content>
          </Card>;
        })}
      </div>
    </section>

    <Card className="practice-mix">
      <Card.Header><Card.Title>Practice mix</Card.Title></Card.Header>
      <Card.Content>
        <div className="practice-levels grid gap-6 sm:grid-cols-2">
          <div className="flex flex-wrap items-end gap-3">
            <NumberField variant="secondary" id="background-reduction" value={options.background_reduction_db} minValue={0} maxValue={60} step={1} isDisabled={busy}
              onChange={(value) => { options.background_reduction_db = value; onOptionsChange(); }}>
              <Label>Background reduction (dB)</Label>
              <NumberField.Group>
                <NumberField.DecrementButton aria-label="Decrease background reduction" />
                <NumberField.Input />
                <NumberField.IncrementButton aria-label="Increase background reduction" />
              </NumberField.Group>
            </NumberField>
            {Number.isFinite(options.background_reduction_db) && options.background_reduction_db !== (defaults.background_reduction_db ?? 6) &&
              <Button variant="secondary" isDisabled={busy} aria-label="Set default background reduction"
                onPress={() => onSetLevelDefault("background_reduction_db")}>Set Default</Button>}
          </div>
          <div className="flex flex-wrap items-end gap-3">
            <NumberField variant="secondary" id="metronome-level" value={options.metronome_db} minValue={-60} maxValue={0} step={1}
              isDisabled={busy || !options.metronome_enabled}
              onChange={(value) => { options.metronome_db = value; onOptionsChange(); }}>
              <Label>Metronome level (dB)</Label>
              <NumberField.Group>
                <NumberField.DecrementButton aria-label="Decrease metronome level" />
                <NumberField.Input />
                <NumberField.IncrementButton aria-label="Increase metronome level" />
              </NumberField.Group>
            </NumberField>
            {Number.isFinite(options.metronome_db) && options.metronome_db !== (defaults.metronome_db ?? -6) &&
              <Button variant="secondary" isDisabled={busy || !options.metronome_enabled} aria-label="Set default metronome level"
                onPress={() => onSetLevelDefault("metronome_db")}>Set Default</Button>}
          </div>
        </div>
        <Description>Foreground stays at 0 dB. Background instruments are reduced by this amount.</Description>
        <Checkbox variant="secondary" id="practice-metronome" isSelected={options.metronome_enabled} isDisabled={busy}
          onChange={(value) => { options.metronome_enabled = value; onOptionsChange(); }}>
          <Checkbox.Content>
            <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
            <Label>Enable metronome</Label>
          </Checkbox.Content>
        </Checkbox>
        <Description>Uses MuseScore’s built-in metronome. No count-in is added.</Description>
      </Card.Content>
    </Card>

    {result && <Card className="practice-results" aria-label="Practice track results">
      <Card.Header><Card.Title>{result.cancelled ? "Rendering cancelled" : result.error ? "Rendering stopped" : "Practice tracks created"}</Card.Title></Card.Header>
      <Card.Content>
        {result.error && <p className="text-danger" role="alert">{result.error}</p>}
        <p className="wrap-anywhere">{result.completed.length} completed {result.completed.length === 1 ? "track" : "tracks"} saved in {result.directory}</p>
        <ul className="list-inside list-disc space-y-2">{result.completed.map((track) => <li className="wrap-anywhere" key={track.key}>{track.name}<Description>{track.path}</Description></li>)}</ul>
      </Card.Content>
    </Card>}
    <footer className="export-bar sticky bottom-4 z-10"><Surface className="flex flex-wrap items-center justify-between gap-4 rounded-3xl p-4 shadow-surface">
      <p>Create MP3 practice tracks.<br /><span>192 kbps · Source score stays intact</span></p>
      <div className="button-row flex flex-wrap gap-3">
        <Button variant="primary" isDisabled={instrumentsDisabled} data-action="practice-export" onPress={onExport}>
          {rendering ? "Rendering…" : "Create Practice Tracks"}
        </Button>
      </div>
    </Surface></footer>
  </div>;
}
