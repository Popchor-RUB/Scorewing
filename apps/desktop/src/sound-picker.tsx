import { Fragment, useId, useRef, useState } from "react";
import { Button } from "./components/ui/button.tsx";
import { Field, FieldError, FieldGroup, FieldLabel } from "./components/ui/field.tsx";
import { Combobox, ComboboxCollection, ComboboxContent, ComboboxEmpty, ComboboxGroup, ComboboxInput, ComboboxItem, ComboboxLabel, ComboboxList, ComboboxSeparator } from "./components/ui/combobox.tsx";
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
  const invalid = !!value && !selected;
  const selectedLabel = selected?.name ?? (value ? `Unavailable sound (${value.bank}:${value.program})` : "");
  const searching = query !== null && query.length > 0;
  const recentPresets = searching ? [] : recent.flatMap((sound) => {
    const preset = presets.find((entry) => soundKey(entry) === soundKey(sound));
    return preset ? [preset] : [];
  });
  const recentKeys = new Set(recentPresets.map(soundKey));
  const catalog = presets.filter((preset) => !recentKeys.has(soundKey(preset)) && (!searching || preset.name.toLocaleLowerCase().includes(query!.toLocaleLowerCase())));
  const groups = [
    ...(recentPresets.length ? [{ label: "Recently selected", items: recentPresets.map(soundKey) }] : []),
    ...(catalog.length ? [{ label: "All instruments", items: catalog.map(soundKey) }] : []),
  ];
  function commit(next: string) {
    const preset = presets.find((entry) => soundKey(entry) === next);
    if (!preset) return;
    remember(preset);
    onChange({ bank: preset.bank, program: preset.program });
    setQuery(null);
    setOpen(false);
  }
  return <FieldGroup className="sound-picker min-w-0 max-w-full">
    <div className="flex flex-wrap items-end gap-2">
      <Field className="min-w-0 flex-1" data-disabled={isDisabled} data-invalid={invalid}>
        <Combobox items={groups} value={key} disabled={isDisabled}
          itemToStringLabel={(itemKey) => presets.find((preset) => soundKey(preset) === itemKey)?.name ?? selectedLabel}
          filter={null} inputValue={query ?? selectedLabel} onInputValueChange={(next, details) => {
            if (details.reason === "input-change") { setQuery(next); setOpen(true); }
          }}
          open={open} onOpenChange={(next) => { setOpen(next); if (!next) setQuery(null); }}
          onValueChange={(next, details) => { if (next === null) details.cancel(); else commit(next); }}>
          <FieldLabel id={`${inputId}-label`} htmlFor={inputId} className="sr-only">{label}{accessibleLabel && <span> · {accessibleLabel}</span>}</FieldLabel>
          <ComboboxInput id={inputId} aria-labelledby={`${inputId}-label`} aria-invalid={invalid} aria-describedby={invalid ? `${inputId}-error` : undefined} placeholder="Choose an instrument" disabled={isDisabled}
            onFocus={(event) => {
              if (focused.current) return;
              focused.current = true;
              setQuery(null); setOpen(true); event.target.select();
            }}
            onBlur={() => { focused.current = false; setQuery(null); setOpen(false); }} />
          <ComboboxContent>
            <ComboboxEmpty>No instruments found</ComboboxEmpty>
            <ComboboxList>{(group: typeof groups[number], index: number) => <Fragment key={group.label}>
              {index > 0 && <ComboboxSeparator />}
              <ComboboxGroup items={group.items}><ComboboxLabel>{group.label}</ComboboxLabel>
                <ComboboxCollection>{(itemKey: string) => <ComboboxItem key={itemKey} value={itemKey} data-key={itemKey}>
                  {presets.find((preset) => soundKey(preset) === itemKey)!.name}
                </ComboboxItem>}</ComboboxCollection>
              </ComboboxGroup>
            </Fragment>}</ComboboxList>
          </ComboboxContent>
        </Combobox>
      </Field>
      {onSetDefault && selected && (!defaultValue || key !== soundKey(defaultValue)) &&
        <Button variant="ghost" size="sm" disabled={isDisabled} aria-label={`Set default ${label.toLocaleLowerCase()}`}
          onClick={() => onSetDefault({ bank: selected.bank, program: selected.program })}>Set Default</Button>}
    </div>
    {invalid && <FieldError id={`${inputId}-error`}>This sound is unavailable in the current catalog. Choose another instrument.</FieldError>}
  </FieldGroup>;
}
