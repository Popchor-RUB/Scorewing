import { Combobox } from "@base-ui/react/combobox";
import { Fragment, useId, useRef, useState } from "react";
import { Button } from "./components/ui/button.tsx";
import type { PresetId, SoundPreset } from "./model";
import { soundKey, useRecentSounds } from "./recent-sounds";

export { RecentSoundsProvider } from "./recent-sounds";

export interface SoundPickerProps {
  id?: string;
  label: string;
  "aria-label"?: string;
  presets: SoundPreset[];
  value: PresetId | null;
  onChange: (value: PresetId | null) => void;
  isDisabled?: boolean;
  defaultValue?: PresetId | null;
  onSetDefault?: (sound: PresetId) => void;
}

export function SoundPicker({ id, label, "aria-label": accessibleLabel, presets, value, onChange, isDisabled, defaultValue, onSetDefault }: SoundPickerProps) {
  const generatedId = useId();
  const inputId = id ?? generatedId;
  const { recent, remember } = useRecentSounds();
  const [query, setQuery] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const focused = useRef(false);
  const key = value ? soundKey(value) : null;
  const selected = presets.find((preset) => soundKey(preset) === key);
  const selectedLabel = selected?.name ?? (value ? `Unavailable sound (${value.bank}:${value.program})` : "");
  const searching = query !== null && query.length > 0;
  const recentPresets = searching ? [] : recent.flatMap((sound) => {
    const preset = presets.find((entry) => soundKey(entry) === soundKey(sound));
    return preset ? [preset] : [];
  });
  const recentKeys = new Set(recentPresets.map(soundKey));
  const catalog = presets.filter((preset) => !recentKeys.has(soundKey(preset)) && (!searching || preset.name.toLocaleLowerCase().includes(query!.toLocaleLowerCase())));
  const visible = [...recentPresets, ...catalog];
  const groups = [
    ...(recentPresets.length ? [{ label: "Recently selected", items: recentPresets.map(soundKey) }] : []),
    ...(catalog.length ? [{ label: "All instruments", items: catalog.map(soundKey) }] : []),
  ];
  function commit(next: string | null) {
    const preset = presets.find((entry) => soundKey(entry) === next);
    if (!preset) return;
    remember(preset);
    onChange({ bank: preset.bank, program: preset.program });
    setQuery(null);
    setOpen(false);
  }
  const item = (preset: SoundPreset) => <Combobox.Item key={soundKey(preset)} value={soundKey(preset)}
    data-key={soundKey(preset)} className="sound-option"
    onClick={() => { if (soundKey(preset) === key) commit(key); }}>
    <span>{preset.name}</span><Combobox.ItemIndicator aria-hidden="true"><svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2"><path d="m3 8 3 3 7-7" /></svg></Combobox.ItemIndicator>
  </Combobox.Item>;

  return <div className="sound-picker min-w-0 max-w-full">
    <div className="flex flex-wrap items-end gap-2">
      <div className="combo-box min-w-0 flex-1">
        <Combobox.Root items={groups} value={key} disabled={isDisabled}
          itemToStringLabel={(itemKey) => presets.find((preset) => soundKey(preset) === itemKey)?.name ?? selectedLabel}
          filter={null} inputValue={query ?? selectedLabel} onInputValueChange={() => {}}
          open={open} onOpenChange={(next) => { setOpen(next); if (!next) setQuery(null); }}
          onValueChange={commit}>
          <label id={`${inputId}-label`} htmlFor={inputId} className="sound-label muted">{label}{accessibleLabel && <span className="sr-only"> · {accessibleLabel}</span>}</label>
          <Combobox.InputGroup className="sound-input-group">
            <Combobox.Input className="input" id={inputId} aria-labelledby={`${inputId}-label`} placeholder="Choose an instrument"
              onChange={(event) => { setQuery(event.target.value); setOpen(true); }}
              onFocus={(event) => {
                if (focused.current) return;
                focused.current = true;
                setQuery(null); setOpen(true); event.target.select();
              }}
              onBlur={() => { focused.current = false; setQuery(null); setOpen(false); }} />
            <Combobox.Trigger aria-label={`Show ${label.toLocaleLowerCase()} instruments`}>⌄</Combobox.Trigger>
          </Combobox.InputGroup>
          <Combobox.Portal>
            <Combobox.Positioner sideOffset={4} className="sound-positioner">
              <Combobox.Popup className="sound-popup">
                {visible.length === 0 && <p className="muted p-3">No instruments found</p>}
                <Combobox.List className="sound-list">{(group: typeof groups[number], index: number) => <Fragment key={group.label}>
                  {index > 0 && <hr data-slot="separator" />}
                  <Combobox.Group items={group.items}><Combobox.GroupLabel className="sound-group-label">{group.label}</Combobox.GroupLabel>
                    <Combobox.Collection>{(itemKey: string) => item(presets.find((preset) => soundKey(preset) === itemKey)!)}</Combobox.Collection>
                  </Combobox.Group>
                </Fragment>}</Combobox.List>
                <p className="sound-shortcuts muted">↑ ↓ Navigate · Enter Choose · Esc Close</p>
              </Combobox.Popup>
            </Combobox.Positioner>
          </Combobox.Portal>
        </Combobox.Root>
      </div>
      {onSetDefault && selected && (!defaultValue || key !== soundKey(defaultValue)) &&
        <Button variant="ghost" size="sm" disabled={isDisabled} aria-label={`Set default ${label.toLocaleLowerCase()}`}
          onClick={() => onSetDefault({ bank: selected.bank, program: selected.program })}>Set Default</Button>}
    </div>
    {value && !selected && <p className="text-danger">This sound is unavailable in the current catalog. Choose another instrument.</p>}
  </div>;
}
