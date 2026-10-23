export interface SplitLane {
  voice: string;
  tone: number;
  fallback_voice: string | null;
}

export interface SplitFallback extends SplitLane {
  part_id: string;
}

export interface Part {
  id: string;
  name: string;
  instrument_names: string[];
  voice_count: number;
  split_plans: Record<"chords" | "voices", SplitLane[]>;
}

export interface Inspection {
  id: string;
  path: string;
  name: string;
  parts: Part[];
  error: string | null;
  format?: "musicxml" | "musescore";
  practice_parts?: PracticePart[];
}

export interface PresetId { bank: number; program: number }
export interface SoundPreset extends PresetId { name: string }
export interface PracticePart {
  key: string;
  name: string;
  visible: boolean;
  original_instruments: string[];
  existing_preset: PresetId | null;
}
export interface PracticePartOptions {
  key: string;
  foreground_sound: PresetId | null;
  background_sound: PresetId | null;
  include_in_background: boolean;
  create_track: boolean;
}
export interface PracticeOptions {
  parts: PracticePartOptions[];
  background_reduction_db: number;
  metronome_enabled: boolean;
  metronome_db: number;
}
export interface PracticeConfig { executable: string | null; soundfont: string | null }
export interface PracticeEnvironment extends PracticeConfig {
  version: string | null;
  presets: SoundPreset[];
  error: string | null;
}
export interface PracticeResult {
  directory: string;
  completed: { key: string; name: string; path: string }[];
  error: string | null;
  cancelled: boolean;
}
export interface PracticeProgress { id: string; stage: string; completed: number; total: number }

export function newPractice(parts: PracticePart[]): PracticeOptions {
  return {
    parts: parts.map((part) => ({
      key: part.key, foreground_sound: part.existing_preset,
      background_sound: part.existing_preset,
      include_in_background: part.visible, create_track: part.visible,
    })),
    background_reduction_db: 6, metronome_enabled: false, metronome_db: -6,
  };
}

export function practiceOptions(options: PracticeOptions, presets: SoundPreset[]): PracticeOptions {
  if (!options.parts.some((part) => part.create_track))
    throw new Error("Choose at least one instrument to create a practice track.");
  if (!Number.isFinite(options.background_reduction_db) || options.background_reduction_db < 0 || options.background_reduction_db > 60)
    throw new Error("Background reduction must be between 0 and 60 dB.");
  if (!Number.isFinite(options.metronome_db) || options.metronome_db < -60 || options.metronome_db > 0)
    throw new Error("Metronome level must be between −60 and 0 dB.");
  const valid = (sound: PresetId | null) => sound !== null && presets.some((preset) => preset.bank === sound.bank && preset.program === sound.program);
  for (const [index, part] of options.parts.entries()) {
    if (part.create_track && !valid(part.foreground_sound))
      throw new Error(`Choose an MS Basic foreground sound for instrument ${index + 1}.`);
    if (part.include_in_background && !valid(part.background_sound))
      throw new Error(`Choose an MS Basic background sound for instrument ${index + 1}.`);
  }
  return structuredClone(options);
}

export type StemDirection = "up" | "down";

export type Operation =
  | {
      kind: "split";
      part_ids: string[];
      mode: "chords" | "voices";
      fallbacks: SplitFallback[];
      insert_fallback_slurs: boolean;
    }
  | { kind: "merge"; groups: string[][]; stems: StemDirection[][] };

export interface Arrangement {
  kind: "split" | "merge" | "practice";
  mode: "chords" | "voices";
  selected: Set<string>;
  groups: string[][];
  stems: Map<string, StemDirection>;
  fallbacks: Record<"chords" | "voices", Map<string, string | null>>;
  insertFallbackSlurs: boolean;
}

export function newArrangement(): Arrangement {
  return {
    kind: "split",
    mode: "chords",
    selected: new Set(),
    groups: [],
    stems: new Map(),
    fallbacks: { chords: new Map(), voices: new Map() },
    insertFallbackSlurs: true,
  };
}

export function stemDirection(
  arrangement: Arrangement,
  part: string,
  index: number,
): StemDirection {
  return arrangement.stems.get(part) ?? (index % 2 === 0 ? "up" : "down");
}

export function fallbackVoice(
  arrangement: Arrangement,
  part: string,
  lane: SplitLane,
): string | null {
  const key = JSON.stringify([part, lane.voice, lane.tone]);
  const choices = arrangement.fallbacks[arrangement.mode];
  return choices.has(key) ? choices.get(key)! : lane.fallback_voice;
}

export function setFallbackVoice(
  arrangement: Arrangement,
  part: string,
  lane: SplitLane,
  voice: string | null,
): void {
  arrangement.fallbacks[arrangement.mode].set(
    JSON.stringify([part, lane.voice, lane.tone]),
    voice,
  );
}

export function addGroup(
  arrangement: Arrangement,
  parts: Part[],
): string | null {
  const assigned = new Set(arrangement.groups.flat());
  const ids = parts
    .filter((p) => arrangement.selected.has(p.id))
    .map((p) => p.id);
  if (ids.length < 2)
    return "Select at least two instruments to create a merge group.";
  if (ids.some((id) => assigned.has(id)))
    return "An instrument can belong to only one merge group.";
  arrangement.groups.push(ids);
  arrangement.selected.clear();
  return null;
}

export function movePart(group: string[], index: number, offset: -1 | 1): void {
  const next = index + offset;
  if (index < 0 || index >= group.length || next < 0 || next >= group.length)
    return;
  [group[index], group[next]] = [group[next], group[index]];
}

export function operation(arrangement: Arrangement, parts: Part[]): Operation {
  if (arrangement.kind === "practice")
    throw new Error("Use practice track export for MuseScore files.");
  const known = new Set(parts.map((part) => part.id));
  if (arrangement.kind === "split") {
    const part_ids = parts
      .filter((p) => arrangement.selected.has(p.id))
      .map((p) => p.id);
    if (!part_ids.length)
      throw new Error("Select at least one instrument to split.");
    const fallbacks = parts
      .filter((part) => arrangement.selected.has(part.id))
      .flatMap((part) =>
        part.split_plans[arrangement.mode].map((lane) => ({
          ...lane,
          part_id: part.id,
          fallback_voice: fallbackVoice(arrangement, part.id, lane),
        })),
      );
    return {
      kind: "split", part_ids, mode: arrangement.mode, fallbacks,
      insert_fallback_slurs: arrangement.insertFallbackSlurs,
    };
  }
  if (!arrangement.groups.length)
    throw new Error("Create at least one merge group.");
  const used = new Set<string>();
  for (const group of arrangement.groups) {
    if (group.length < 2)
      throw new Error("Each merge group needs at least two instruments.");
    for (const id of group) {
      if (!known.has(id) || used.has(id))
        throw new Error(
          "Merge groups must contain distinct instruments from this score.",
        );
      used.add(id);
    }
  }
  return {
    kind: "merge",
    groups: arrangement.groups.map((group) => [...group]),
    stems: arrangement.groups.map((group) =>
      group.map((id, index) => stemDirection(arrangement, id, index)),
    ),
  };
}
