import assert from "node:assert/strict";
import test from "node:test";
import type { Inspection, Operation } from "./model.ts";
import { mountTestApp } from "./test-support.ts";

function score(): Inspection {
  return {
    id: "arrangement", path: "/scores/arrangement.xml", name: "arrangement.xml", error: null, format: "musicxml",
    parts: ["P1", "P2", "P3", "P4"].map((id, index) => ({
      id, name: ["Soprano", "Alto", "Tenor", "Bass"][index], instrument_names: [], voice_count: 2,
      split_plans: {
        chords: [
          { voice: "1", tone: 0, fallback_voice: "2" },
          { voice: "1", tone: 1, fallback_voice: "2" },
          { voice: "2", tone: 0, fallback_voice: "1" },
        ],
        voices: [
          { voice: "1", tone: 0, fallback_voice: "2" },
          { voice: "2", tone: 0, fallback_voice: "1" },
        ],
      },
    })),
  };
}

async function arrangementApp() {
  const exported: Operation[] = [];
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return ["/scores/arrangement.xml"];
    if (command === "inspect_file") return score();
    if (command === "release_document") return;
    if (command === "export_document") { exported.push(args.operation as Operation); return null; }
    throw new Error(`Unexpected command ${command}`);
  });
  await app.menu("open-score");
  return { app, exported };
}

test("Split updates planned-staff counts globally and isolates first-staff None by mode, source and chord tone", async () => {
  const { app, exported } = await arrangementApp();
  try {
    await app.click('[data-mode="split"]');
    assert.deepEqual(Array.from(app.document.querySelectorAll(".source-count"), (element) => element.textContent), Array(4).fill("3 new staves"));
    await app.click('[data-part="P1"]');
    await app.click('[data-part="P2"]');
    assert.equal(app.document.querySelectorAll(".fallback-row").length, 6);
    assert.match(app.find(".workspace-footer").textContent!, /2 source staves selected · 6 new staves/);
    assert.equal(app.find<HTMLButtonElement>("#fallback-P1-1-0").disabled, false, "first generated staff is configurable");
    await app.click("#fallback-P1-1-0");
    assert.deepEqual(Array.from(app.document.querySelectorAll('[role="option"]'), (option) => option.textContent?.replace("✓", "").trim()), ["None (silent)", "Voice 2"], "exclude the generated lane's own voice");
    await app.click('[role="option"]');
    await app.click('[data-action="export"]');
    const chords = exported.at(-1);
    assert.ok(chords?.kind === "split");
    assert.deepEqual(chords.fallbacks.map(({ part_id, voice, tone, fallback_voice }) => [part_id, voice, tone, fallback_voice]), [
      ["P1", "1", 0, null], ["P1", "1", 1, "2"], ["P1", "2", 0, "1"],
      ["P2", "1", 0, "2"], ["P2", "1", 1, "2"], ["P2", "2", 0, "1"],
    ]);
    await app.choose("#split-mode", "Explicit voices");
    assert.deepEqual(Array.from(app.document.querySelectorAll(".source-count"), (element) => element.textContent), Array(4).fill("2 new staves"));
    assert.equal(app.document.querySelectorAll(".fallback-row").length, 4);
    assert.match(app.find(".workspace-footer").textContent!, /4 new staves/);
    assert.doesNotMatch(app.find(".configuration-table").textContent!, /Chord tone/);
    assert.match(app.find("#fallback-P1-1-0").textContent!, /Voice 2/, "voice mode retains its default despite the chord-mode override");
    await app.choose("#fallback-P2-1-0", "None (silent)");
    await app.click('[data-action="export"]');
    const voices = exported.at(-1);
    assert.ok(voices?.kind === "split");
    assert.deepEqual(voices.fallbacks.map(({ fallback_voice }) => fallback_voice), ["2", "1", null, "1"]);
    await app.choose("#split-mode", "Chord tones");
    await app.click('[data-part="P1"]');
    await app.click('[data-part="P1"]');
    await app.click('[data-action="export"]');
    assert.deepEqual(exported.at(-1), chords, "switching modes and reselecting a source preserves explicit None");
  } finally { await app.close(); }
});

test("Merge stems stay attached to source IDs while implicit defaults follow whole-group reordering", async () => {
  const { app, exported } = await arrangementApp();
  try {
    await app.click('[data-mode="merge"]');
    for (const id of ["P1", "P2", "P3"]) await app.click(`[data-part="${id}"]`);
    await app.click('[data-action="add-group"]');
    assert.match(app.find(".merge-create").textContent!, /No instruments selected/);
    for (const id of ["P1", "P2", "P3"]) {
      const source = app.find(`[data-part="${id}"]`);
      assert.equal(source.getAttribute("aria-disabled"), "true", `${id} cannot join another group`);
      assert.equal(source.getAttribute("aria-checked"), "false", "group creation clears source selection");
    }
    assert.equal(app.document.querySelectorAll('[id^="stem-"]').length, 3, "one inline selector per source");
    assert.equal(app.find('[aria-label="Source order"]').querySelector('[role="combobox"]'), null);
    await app.choose("#stem-P1", "Down");
    await app.click('[data-action="up"][data-group="0"][data-index="2"]');
    assert.deepEqual(Array.from(app.document.querySelectorAll(".merge-part [id^='stem-']"), (element) => element.id), ["stem-P1", "stem-P3", "stem-P2"]);
    await app.click('[data-action="export"]');
    assert.deepEqual(exported.at(-1), { kind: "merge", groups: [["P1", "P3", "P2"]], stems: [["down", "down", "up"]] });
    await app.click('[data-action="down"][data-group="0"][data-index="0"]');
    await app.click('[data-action="export"]');
    assert.deepEqual(exported.at(-1), { kind: "merge", groups: [["P3", "P1", "P2"]], stems: [["up", "down", "up"]] });
    await app.click('[data-action="remove-group"]');
    assert.equal(app.document.querySelectorAll(".merge-part").length, 0);
    assert.notEqual(app.find('[data-part="P1"]').getAttribute("aria-disabled"), "true", "removal makes sources available again");
  } finally { await app.close(); }
});
