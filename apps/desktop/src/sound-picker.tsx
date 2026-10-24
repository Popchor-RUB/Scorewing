import { Button, ComboBox, Description, Header, Input, Label, ListBox, Separator } from "@heroui/react";
import { useRef, useState } from "react";
import type { PresetId, SoundPreset } from "./model";
import { soundKey, useRecentSounds } from "./recent-sounds";

export { RecentSoundsProvider } from "./recent-sounds";

export interface SoundPickerProps {
  id?: string;
  label: string;
  presets: SoundPreset[];
  value: PresetId | null;
  onChange: (value: PresetId | null) => void;
  isDisabled?: boolean;
}

export function SoundPicker({ id, label, presets, value, onChange, isDisabled }: SoundPickerProps) {
  const { recent, remember } = useRecentSounds();
  const [query, setQuery] = useState<string | null>(null);
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
  const item = (preset: SoundPreset) => (
    <ListBox.Item key={soundKey(preset)} id={soundKey(preset)} textValue={preset.name} onAction={() => {
      remember(preset);
      onChange({ bank: preset.bank, program: preset.program });
      setQuery(null);
    }}>
      <Label>{preset.name}</Label>
      <ListBox.ItemIndicator />
    </ListBox.Item>
  );

  return <div className="sound-picker min-w-0 space-y-2">
    <ComboBox
      className="w-full"
      variant="secondary"
      id={id}
      isDisabled={isDisabled}
      selectedKey={key}
      inputValue={query ?? selectedLabel}
      // Only typed input filters; Aria also resets input text when opening and closing.
      onInputChange={() => {}}
      defaultFilter={() => true}
      menuTrigger="focus"
      allowsEmptyCollection
      onOpenChange={(isOpen) => { if (!isOpen) setQuery(null); }}
    >
      <Label>{label}</Label>
      <ComboBox.InputGroup>
        <Input
          placeholder="Choose an instrument"
          onChange={(event) => setQuery(event.target.value)}
          onFocus={(event) => {
            if (focused.current) return;
            focused.current = true;
            setQuery(null);
            event.target.select();
          }}
          onBlur={() => { focused.current = false; setQuery(null); }}
        />
        <ComboBox.Trigger aria-label={`Show ${label.toLocaleLowerCase()} instruments`} />
      </ComboBox.InputGroup>
      <ComboBox.Popover>
        <ListBox
          renderEmptyState={() => <span className="p-5 text-muted">No instruments found</span>}
        >
          {recentPresets.length > 0 && <ListBox.Section id="recent"><Header>Recently selected</Header>{recentPresets.map(item)}</ListBox.Section>}
          {recentPresets.length > 0 && catalog.length > 0 && <Separator />}
          {catalog.length > 0 && <ListBox.Section id="catalog"><Header>All instruments</Header>{catalog.map(item)}</ListBox.Section>}
        </ListBox>
      </ComboBox.Popover>
    </ComboBox>
    {value && <Button variant="secondary" size="sm" isDisabled={isDisabled} onPress={() => { onChange(null); setQuery(null); }} aria-label={`Clear ${label.toLocaleLowerCase()}`}>Clear</Button>}
    {value && !selected && <Description className="text-danger">This sound is unavailable in the current catalog. Choose an instrument or clear it.</Description>}
  </div>;
}
