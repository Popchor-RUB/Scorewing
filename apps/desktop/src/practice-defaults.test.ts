import assert from "node:assert/strict";
import test from "node:test";
import { newPractice } from "./model.ts";
import { PRACTICE_DEFAULTS_KEY, readPracticeDefaults, savePracticeDefaults } from "./practice-defaults.ts";

test("practice defaults persist validated sounds and mix settings and tolerate unavailable storage", () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  let stored: string | null = null;
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: {
    getItem(key: string) { assert.equal(key, PRACTICE_DEFAULTS_KEY); return stored; },
    setItem(key: string, value: string) { assert.equal(key, PRACTICE_DEFAULTS_KEY); stored = value; },
  } });
  try {
    assert.deepEqual(readPracticeDefaults(), {});
    const defaults = {
      foreground_sound: { bank: 0, program: 0 }, background_sound: { bank: 1, program: 40 },
      background_reduction_db: 12, metronome_enabled: true, metronome_db: -15,
    };
    savePracticeDefaults(defaults);
    assert.deepEqual(readPracticeDefaults(), defaults);
    const options = newPractice([
      { key: "one", name: "One", visible: true, original_instruments: [], existing_preset: { bank: 0, program: 20 } },
      { key: "two", name: "Two", visible: false, original_instruments: [], existing_preset: null },
    ], readPracticeDefaults());
    for (const part of options.parts) {
      assert.deepEqual(part.foreground_sound, defaults.foreground_sound);
      assert.deepEqual(part.background_sound, defaults.background_sound);
    }
    assert.equal(options.parts[1].create_track, false, "defaults preserve score visibility");
    assert.equal(options.background_reduction_db, 12);
    assert.equal(options.metronome_db, -15);
    assert.equal(options.metronome_enabled, true);
    stored = JSON.stringify({ foreground_sound: { bank: -1, program: 0 }, background_sound: { bank: 0, program: 256 }, background_reduction_db: 61, metronome_enabled: "true", metronome_db: -61 });
    assert.deepEqual(readPracticeDefaults(), {});
    stored = JSON.stringify({ foreground_sound: { bank: 65535, program: 255, name: "Extra data" }, background_reduction_db: 0, metronome_db: 0, metronome_enabled: false });
    assert.deepEqual(readPracticeDefaults(), { foreground_sound: { bank: 65535, program: 255 }, background_reduction_db: 0, metronome_db: 0, metronome_enabled: false });
    for (const invalid of ["bad json", "null", "[]", "42"]) {
      stored = invalid;
      assert.deepEqual(readPracticeDefaults(), {});
    }
    Object.defineProperty(globalThis, "localStorage", { configurable: true, get() { throw new Error("Storage blocked"); } });
    assert.deepEqual(readPracticeDefaults(), {});
    assert.doesNotThrow(() => savePracticeDefaults(defaults));
  } finally {
    if (original) Object.defineProperty(globalThis, "localStorage", original);
    else Reflect.deleteProperty(globalThis, "localStorage");
  }
});
