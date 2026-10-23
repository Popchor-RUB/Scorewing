import assert from "node:assert/strict";
import test from "node:test";
import { newPractice, practiceOptions, type PracticePart, type SoundPreset } from "./model.ts";

const presets: SoundPreset[] = [
  { bank: 0, program: 0, name: "Piano" },
  { bank: 0, program: 40, name: "Violin" },
];
const parts: PracticePart[] = [
  { key: "first", name: "Violin", visible: true, original_instruments: ["Violin"], existing_preset: presets[1] },
  { key: "second", name: "Violin", visible: false, original_instruments: ["Violin"], existing_preset: null },
];

test("practice defaults follow visibility, identify duplicate names by key, and stay per document", () => {
  const state = newPractice(parts);
  assert.deepEqual(state.parts.map((part) => [part.key, part.include_in_background, part.create_track]), [
    ["first", true, true], ["second", false, false],
  ]);
  assert.equal(state.background_reduction_db, 6);
  assert.equal(state.metronome_enabled, false);
  assert.equal(state.metronome_db, -6);
  state.parts[0].include_in_background = false;
  state.parts[0].background_sound = null;
  state.parts[0].foreground_sound = presets[0];
  const request = practiceOptions(state, presets);
  assert.deepEqual(request.parts[0].foreground_sound, presets[0]);
  assert.equal(newPractice(parts).parts[0].include_in_background, true);
  request.parts[0].create_track = false;
  assert.equal(state.parts[0].create_track, true, "request is an independent snapshot");
});

test("practice validates active sounds, target selection, and finite level limits", () => {
  const state = newPractice(parts);
  state.parts[0].create_track = false;
  assert.throws(() => practiceOptions(state, presets), /at least one instrument/);
  state.parts[0].create_track = true;
  state.parts[1].create_track = true;
  assert.throws(() => practiceOptions(state, presets), /foreground sound for instrument 2/);
  state.parts[1].foreground_sound = presets[0];
  state.parts[1].include_in_background = true;
  assert.throws(() => practiceOptions(state, presets), /background sound for instrument 2/);
  state.parts[1].background_sound = { bank: 9, program: 2 };
  assert.throws(() => practiceOptions(state, presets), /background sound/);
  state.parts[1].background_sound = presets[1];
  for (const invalid of [NaN, Infinity, -1, 61]) {
    state.background_reduction_db = invalid;
    assert.throws(() => practiceOptions(state, presets), /Background reduction/);
  }
  state.background_reduction_db = 60;
  for (const invalid of [NaN, -Infinity, -61, 1]) {
    state.metronome_db = invalid;
    assert.throws(() => practiceOptions(state, presets), /Metronome level/);
  }
  state.metronome_db = -60;
  assert.doesNotThrow(() => practiceOptions(state, presets));
});
