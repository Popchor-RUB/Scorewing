import assert from "node:assert/strict";
import test from "node:test";
import {
  addGroup,
  movePart,
  newArrangement,
  operation,
  type Part,
} from "./model.ts";

const parts: Part[] = ["P1", "P2", "P3", "P4"].map((id) => ({
  id,
  name: "Violin",
  instrument_names: ["Violin"],
  voice_count: 1,
}));

test("split uses stable IDs in score order and refuses an empty selection", () => {
  const state = newArrangement();
  assert.throws(() => operation(state, parts), /Select at least one/);
  state.selected.add("P3").add("P1");
  assert.deepEqual(operation(state, parts), {
    kind: "split",
    mode: "chords",
    part_ids: ["P1", "P3"],
  });
  state.mode = "voices";
  assert.equal((operation(state, parts) as { mode: string }).mode, "voices");
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
