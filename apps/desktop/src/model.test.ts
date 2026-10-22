import assert from "node:assert/strict";
import test from "node:test";
import {
  addGroup,
  fallbackVoice,
  movePart,
  newArrangement,
  operation,
  setFallbackVoice,
  type Part,
} from "./model.ts";

const parts: Part[] = ["P1", "P2", "P3", "P4"].map((id) => ({
  id,
  name: "Violin",
  instrument_names: ["Violin"],
  voice_count: 1,
  split_plans: { chords: [], voices: [] },
}));

test("split uses stable IDs in score order and refuses an empty selection", () => {
  const state = newArrangement();
  assert.throws(() => operation(state, parts), /Select at least one/);
  state.selected.add("P3").add("P1");
  assert.deepEqual(operation(state, parts), {
    kind: "split",
    mode: "chords",
    part_ids: ["P1", "P3"],
    fallbacks: [],
  });
  state.mode = "voices";
  assert.equal((operation(state, parts) as { mode: string }).mode, "voices");
});

test("split serializes every lane and retains explicit None per document and mode", () => {
  const lanes = [
    { voice: "1", tone: 0, fallback_voice: null },
    { voice: "2", tone: 0, fallback_voice: null },
    { voice: "3", tone: 0, fallback_voice: "2" },
  ];
  const chordLanes = [lanes[0], lanes[1], { voice: "2", tone: 1, fallback_voice: null }];
  const score: Part[] = [{ ...parts[0], split_plans: { voices: lanes, chords: chordLanes } }];
  const state = newArrangement();
  state.mode = "voices";
  state.selected.add("P1");
  assert.equal(fallbackVoice(state, "P1", lanes[2]), "2");
  setFallbackVoice(state, "P1", lanes[2], null);
  state.selected.clear();
  state.mode = "chords";
  assert.equal(fallbackVoice(state, "P1", lanes[2]), "2");
  setFallbackVoice(state, "P1", chordLanes[2], "3");
  state.selected.add("P1");
  const chordRequest = operation(state, score);
  assert.equal(chordRequest.kind, "split");
  if (chordRequest.kind !== "split") return;
  assert.equal(chordRequest.fallbacks.length, 3);
  assert.deepEqual(chordRequest.fallbacks[2], { part_id: "P1", voice: "2", tone: 1, fallback_voice: "3" });
  state.mode = "voices";
  assert.deepEqual(operation(state, score), {
    kind: "split",
    mode: "voices",
    part_ids: ["P1"],
    fallbacks: lanes.map((lane) => ({ ...lane, part_id: "P1", fallback_voice: null })),
  });
  const otherDocument = newArrangement();
  otherDocument.mode = "voices";
  assert.equal(fallbackVoice(otherDocument, "P1", lanes[2]), "2");
});

test("merge groups are disjoint, ordered, and require two parts", () => {
  const state = newArrangement();
  state.kind = "merge";
  state.selected.add("P1");
  assert.match(addGroup(state, parts)!, /at least two/);
  state.selected.add("P2");
  assert.equal(addGroup(state, parts), null);
  assert.equal(state.selected.size, 0);
  movePart(state.groups[0], 1, -1);
  movePart(state.groups[0], 0, -1);
  state.selected.add("P1").add("P3");
  assert.match(addGroup(state, parts)!, /only one/);
  state.selected = new Set(["P3", "P4"]);
  assert.equal(addGroup(state, parts), null);
  assert.deepEqual(operation(state, parts), {
    kind: "merge",
    groups: [
      ["P2", "P1"],
      ["P3", "P4"],
    ],
  });
});

test("export catches invalid or overlapping merge groups", () => {
  const state = newArrangement();
  state.kind = "merge";
  assert.throws(() => operation(state, parts), /Create at least one/);
  state.groups = [
    ["P1", "P2"],
    ["P2", "P3"],
  ];
  assert.throws(() => operation(state, parts), /distinct/);
  state.groups = [["P1", "missing"]];
  assert.throws(() => operation(state, parts), /distinct/);
});
