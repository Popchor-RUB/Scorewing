import { Button, Card, Checkbox, Input, Label, NumberField, ProgressBar, TextField } from "@heroui/react";
import { useEffect, useState } from "react";
import type {
  PracticeConfig, PracticeEnvironment, PracticeOptions, PracticePart,
  PracticeProgress, PracticeResult,
} from "./model.ts";
import { SoundPicker } from "./sound-picker.tsx";

export interface PracticeViewProps {
  parts: PracticePart[];
  options: PracticeOptions;
  config: PracticeConfig;
  environment: PracticeEnvironment | null;
  checking: boolean;
  busy: boolean;
  rendering: boolean;
  cancelling: boolean;
  progress?: PracticeProgress;
  result?: PracticeResult;
  onOptionsChange: () => void;
  onConfigChange: (kind: keyof PracticeConfig, value: string) => void;
  onBrowse: (kind: keyof PracticeConfig) => void;
  onDetect: () => void;
  onExport: () => void;
  onCancel: () => void;
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
  return <div className="path-row">
    <TextField className="practice-path" value={draft} onChange={setDraft} isDisabled={disabled}
      onBlur={() => { if (draft !== (value ?? "")) onChange(kind, draft); }}>
      <Label>{label}</Label>
      <Input placeholder={placeholder} />
    </TextField>
    <Button variant="secondary" isDisabled={disabled} aria-label={`Browse ${label}`} data-action="practice-browse" data-kind={kind} onPress={() => onBrowse(kind)}>Browse</Button>
  </div>;
}

export function PracticeView({
  parts, options, config, environment, checking, busy, rendering, cancelling,
  progress, result, onOptionsChange, onConfigChange, onBrowse, onDetect, onExport, onCancel,
}: PracticeViewProps) {
  const disabled = busy || checking;
  return <div className="practice-view">
    <Card className="practice-environment" aria-label="MuseScore renderer">
      <Card.Header>
        <Card.Title>MuseScore renderer</Card.Title>
        <Card.Description>MuseScore 4.7.x and its matching MS Basic SoundFont are required for MP3 export. You can inspect scores before configuring them.</Card.Description>
      </Card.Header>
      <Card.Content>
        {(["executable", "soundfont"] as const).map((kind) => <RendererPath key={kind}
          kind={kind} value={config[kind]} placeholder={environment?.[kind] ?? "Detect automatically"}
          disabled={disabled} onChange={onConfigChange} onBrowse={onBrowse} />)}
        <Button variant="secondary" isDisabled={disabled} data-action="practice-detect" onPress={onDetect}>
          {checking ? "Checking renderer…" : "Check renderer and sounds"}
        </Button>
        <p className="hint">Check renderer and sounds after changing a path.</p>
        {environment?.version && <p className="hint">Detected MuseScore {environment.version}</p>}
        {environment?.error && <p className="error-text" role="alert">{environment.error}</p>}
        <p className="hint">Keep MuseScore closed during rendering. VoiceWeaver temporarily controls the built-in metronome preference and restores it when rendering ends.</p>
      </Card.Content>
    </Card>

    <section aria-label="Practice instruments">
      <div className="section-heading">
        <h2>Practice instruments</h2>
        <p>The featured instrument uses its foreground sound. Other selected background instruments play more quietly.</p>
      </div>
      <div className="practice-parts">
        {parts.map((part, index) => {
          const setting = options.parts[index];
          const label = `${part.name || "Unnamed instrument"} · Instrument ${index + 1}`;
          return <Card className="practice-part" key={part.key}>
            <Card.Header>
              <Card.Title>{label}{!part.visible && <span className="muted"> · Hidden in score</span>}</Card.Title>
              <Card.Description>{part.original_instruments.join(", ") || "Unnamed instrument"}</Card.Description>
            </Card.Header>
            <Card.Content>
              <div className="practice-membership">
                <Checkbox isSelected={setting.include_in_background} isDisabled={busy}
                  data-practice-index={index} data-practice-field="include_in_background"
                  onChange={(value) => { setting.include_in_background = value; onOptionsChange(); }}>
                  <Checkbox.Content>
                    <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                    <Label>Include in background<span className="sr-only"> · {label}</span></Label>
                  </Checkbox.Content>
                </Checkbox>
                <Checkbox isSelected={setting.create_track} isDisabled={busy}
                  data-practice-index={index} data-practice-field="create_track"
                  onChange={(value) => { setting.create_track = value; onOptionsChange(); }}>
                  <Checkbox.Content>
                    <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                    <Label>Create practice track<span className="sr-only"> · {label}</span></Label>
                  </Checkbox.Content>
                </Checkbox>
              </div>
              <div className="practice-sounds">
                {(["foreground_sound", "background_sound"] as const).map((field) => <SoundPicker key={field} id={`${field}-${index}`}
                  label={`${field === "foreground_sound" ? "Foreground sound" : "Background sound"} · ${label}`}
                  presets={environment?.presets ?? []} value={setting[field]} isDisabled={disabled}
                  onChange={(value) => { setting[field] = value; onOptionsChange(); }} />)}
              </div>
            </Card.Content>
          </Card>;
        })}
      </div>
    </section>

    <Card className="practice-mix">
      <Card.Header><Card.Title>Practice mix</Card.Title></Card.Header>
      <Card.Content>
        <div className="practice-levels">
          <NumberField id="background-reduction" value={options.background_reduction_db} minValue={0} maxValue={60} step={1} isDisabled={busy}
            onChange={(value) => { options.background_reduction_db = value; onOptionsChange(); }}>
            <Label>Background reduction (dB)</Label>
            <NumberField.Group>
              <NumberField.DecrementButton aria-label="Decrease background reduction" />
              <NumberField.Input />
              <NumberField.IncrementButton aria-label="Increase background reduction" />
            </NumberField.Group>
          </NumberField>
          <NumberField id="metronome-level" value={options.metronome_db} minValue={-60} maxValue={0} step={1}
            isDisabled={busy || !options.metronome_enabled}
            onChange={(value) => { options.metronome_db = value; onOptionsChange(); }}>
            <Label>Metronome level (dB)</Label>
            <NumberField.Group>
              <NumberField.DecrementButton aria-label="Decrease metronome level" />
              <NumberField.Input />
              <NumberField.IncrementButton aria-label="Increase metronome level" />
            </NumberField.Group>
          </NumberField>
        </div>
        <p className="hint">Foreground stays at 0 dB. Background instruments are reduced by this amount.</p>
        <Checkbox id="practice-metronome" isSelected={options.metronome_enabled} isDisabled={busy}
          onChange={(value) => { options.metronome_enabled = value; onOptionsChange(); }}>
          <Checkbox.Content>
            <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
            <Label>Enable metronome</Label>
          </Checkbox.Content>
        </Checkbox>
        <p className="hint">Uses MuseScore’s built-in metronome. No count-in is added.</p>
      </Card.Content>
    </Card>

    {progress && <div className="practice-progress" role="status">
      <p>{progress.stage} · {progress.completed} / {progress.total} tracks completed</p>
      <ProgressBar aria-label="Practice track rendering" value={progress.completed} maxValue={Math.max(progress.total, 1)}>
        <ProgressBar.Track><ProgressBar.Fill /></ProgressBar.Track>
      </ProgressBar>
    </div>}
    {result && <Card className="practice-results" aria-label="Practice track results">
      <Card.Header><Card.Title>{result.cancelled ? "Rendering cancelled" : result.error ? "Rendering stopped" : "Practice tracks created"}</Card.Title></Card.Header>
      <Card.Content>
        {result.error && <p className="error-text" role="alert">{result.error}</p>}
        <p>{result.completed.length} completed {result.completed.length === 1 ? "track" : "tracks"} saved in {result.directory}</p>
        <ul>{result.completed.map((track) => <li key={track.key}>{track.name}<small>{track.path}</small></li>)}</ul>
      </Card.Content>
    </Card>}
    <footer className="export-bar">
      <p>Create MP3 practice tracks.<br /><span>192 kbps · Source score stays intact</span></p>
      <div className="button-row">
        {rendering && <Button variant="secondary" isDisabled={cancelling} data-action="practice-cancel" onPress={onCancel}>
          {cancelling ? "Cancelling…" : "Cancel rendering"}
        </Button>}
        <Button variant="primary" isDisabled={disabled} data-action="practice-export" onPress={onExport}>
          {rendering ? "Rendering…" : "Create Practice Tracks"}
        </Button>
      </div>
    </footer>
  </div>;
}
