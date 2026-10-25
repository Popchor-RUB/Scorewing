import { Button, Card, Checkbox, Description, Input, Label, NumberField, Surface, TextField } from "@heroui/react";
import { useEffect, useState } from "react";
import type {
  PracticeConfig, PracticeEnvironment, PracticeOptions, PracticePart,
  PracticeResult,
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
  parts, options, config, environment, checking, busy, rendering,
  result, onOptionsChange, onConfigChange, onBrowse, onDetect, onExport,
}: PracticeViewProps) {
  const disabled = busy || checking;
  return <div className="practice-view grid gap-6">
    <Card className="practice-environment" aria-label="MuseScore renderer">
      <Card.Header>
        <Card.Title>MuseScore renderer</Card.Title>
        <Card.Description>MuseScore 4.7.x and its matching MS Basic SoundFont are required for MP3 export. You can inspect scores before configuring them.</Card.Description>
      </Card.Header>
      <Card.Content className="flex flex-col gap-3">
        {(["executable", "soundfont"] as const).map((kind) => <RendererPath key={kind}
          kind={kind} value={config[kind]} placeholder={environment?.[kind] ?? "Detect automatically"}
          disabled={disabled} onChange={onConfigChange} onBrowse={onBrowse} />)}
        <Button className="self-start" variant="secondary" isDisabled={disabled} data-action="practice-detect" onPress={onDetect}>
          {checking ? "Checking renderer…" : "Check renderer and sounds"}
        </Button>
        <Description>Check renderer and sounds after changing a path.</Description>
        {environment?.version && <Description>Detected MuseScore {environment.version}</Description>}
        {environment?.error && <p className="text-danger" role="alert">{environment.error}</p>}
        <Description>Keep MuseScore closed during rendering. VoiceWeaver temporarily controls the built-in metronome preference and restores it when rendering ends.</Description>
      </Card.Content>
    </Card>

    <section aria-label="Practice instruments">
      <div className="section-heading mb-4 space-y-2">
        <h2 className="text-xl font-semibold">Practice instruments</h2>
        <Description>The featured instrument uses its foreground sound. Other selected background instruments play more quietly.</Description>
      </div>
      <div className="practice-parts grid gap-4">
        {parts.map((part, index) => {
          const setting = options.parts[index];
          const label = `${part.name || "Unnamed instrument"} · Instrument ${index + 1}`;
          return <Card className="practice-part" key={part.key}>
            <Card.Header>
              <Card.Title>{label}{!part.visible && <span className="text-muted"> · Hidden in score</span>}</Card.Title>
              <Card.Description>{part.original_instruments.join(", ") || "Unnamed instrument"}</Card.Description>
            </Card.Header>
            <Card.Content>
              <div className="practice-membership mb-5 flex flex-wrap gap-4">
                <Checkbox variant="secondary" isSelected={setting.include_in_background} isDisabled={busy}
                  data-practice-index={index} data-practice-field="include_in_background"
                  onChange={(value) => { setting.include_in_background = value; onOptionsChange(); }}>
                  <Checkbox.Content>
                    <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                    <Label>Include in background<span className="sr-only"> · {label}</span></Label>
                  </Checkbox.Content>
                </Checkbox>
                <Checkbox variant="secondary" isSelected={setting.create_track} isDisabled={busy}
                  data-practice-index={index} data-practice-field="create_track"
                  onChange={(value) => { setting.create_track = value; onOptionsChange(); }}>
                  <Checkbox.Content>
                    <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
                    <Label>Create practice track<span className="sr-only"> · {label}</span></Label>
                  </Checkbox.Content>
                </Checkbox>
              </div>
              <div className="practice-sounds grid gap-6 md:grid-cols-2">
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
        <div className="practice-levels grid gap-6 sm:grid-cols-2">
          <NumberField variant="secondary" id="background-reduction" value={options.background_reduction_db} minValue={0} maxValue={60} step={1} isDisabled={busy}
            onChange={(value) => { options.background_reduction_db = value; onOptionsChange(); }}>
            <Label>Background reduction (dB)</Label>
            <NumberField.Group>
              <NumberField.DecrementButton aria-label="Decrease background reduction" />
              <NumberField.Input />
              <NumberField.IncrementButton aria-label="Increase background reduction" />
            </NumberField.Group>
          </NumberField>
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
    <footer className="export-bar sticky bottom-4 z-10"><Surface className="flex flex-wrap items-center justify-between gap-4 p-4">
      <p>Create MP3 practice tracks.<br /><span>192 kbps · Source score stays intact</span></p>
      <div className="button-row flex flex-wrap gap-3">
        <Button variant="primary" isDisabled={disabled} data-action="practice-export" onPress={onExport}>
          {rendering ? "Rendering…" : "Create Practice Tracks"}
        </Button>
      </div>
    </Surface></footer>
  </div>;
}
