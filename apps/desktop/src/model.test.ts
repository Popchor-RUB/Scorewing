import assert from "node:assert/strict";
import test from "node:test";
import {
  addGroup,
  chooseArrangementTool,
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
    insert_fallback_slurs: true,
  });
  state.mode = "voices";
  assert.equal((operation(state, parts) as { mode: string }).mode, "voices");
  state.insertFallbackSlurs = false;
  const request = operation(state, parts);
  assert.equal(request.kind === "split" && request.insert_fallback_slurs, false);
  assert.equal(newArrangement().insertFallbackSlurs, true);
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
    insert_fallback_slurs: true,
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
    stems: [["up", "down"], ["up", "down"]],
  });
});

test("merge stem choices follow instruments across reordering and stay per document", () => {
  const state = newArrangement();
  state.kind = "merge";
  state.groups = [["P1", "P2"]];
  state.stems.set("P2", "up");
  assert.deepEqual(operation(state, parts), {
    kind: "merge", groups: [["P1", "P2"]], stems: [["up", "up"]],
  });
  movePart(state.groups[0], 1, -1);
  assert.deepEqual(operation(state, parts), {
    kind: "merge", groups: [["P2", "P1"]], stems: [["up", "down"]],
  });
  const other = newArrangement();
  other.kind = "merge";
  other.groups = [["P1", "P2"]];
  assert.deepEqual(operation(other, parts), {
    kind: "merge", groups: [["P1", "P2"]], stems: [["up", "down"]],
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

test("MuseScore visibility initializes each tool once and remembers an explicit clear", () => {
  const sources = parts.map((part, index) => ({ ...part, visible: index !== 1, staff_count: index === 3 ? 2 : 1 }));
  const state = newArrangement();
  chooseArrangementTool(state, "split", sources, true);
  assert.deepEqual([...state.selected], ["P1", "P3"]);
  state.selected = new Set();
  chooseArrangementTool(state, "merge", sources, true);
  assert.deepEqual([...state.selected], ["P1", "P3"]);
  state.selected.add("P2");
  chooseArrangementTool(state, "practice", sources, true);
  chooseArrangementTool(state, "split", sources, true);
  assert.deepEqual([...state.selected], []);
  chooseArrangementTool(state, "merge", sources, true);
  assert.deepEqual([...state.selected], ["P1", "P3", "P2"]);
  const next = newArrangement();
  chooseArrangementTool(next, "split", sources, true);
  assert.deepEqual([...next.selected], ["P1", "P3"]);
  const musicxml = newArrangement();
  chooseArrangementTool(musicxml, "split", sources, false);
  assert.deepEqual([...musicxml.selected], [], "plain MusicXML retains its existing defaults");
});

test("unsupported sources cannot enter split requests or merge groups", () => {
  for (const unsupported of [{ staff_count: 2 }, { supported: false }]) {
    const sources = [{ ...parts[0], ...unsupported }, parts[1]];
    const state = newArrangement();
    state.selected.add("P1").add("P2");
    assert.throws(() => operation(state, sources), /unsupported/);
    state.kind = "merge";
    assert.match(addGroup(state, sources)!, /unsupported/);
    assert.equal(state.groups.length, 0);
    state.groups = [["P1", "P2"]];
    assert.throws(() => operation(state, sources), /unsupported/);
  }
});
