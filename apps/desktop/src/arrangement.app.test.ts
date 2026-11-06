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

async function arrangementApp(document = score()) {
  const exported: Operation[] = [];
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return ["/scores/arrangement.xml"];
    if (command === "inspect_file") return document;
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
    assert.equal(app.document.querySelectorAll('[data-part][aria-checked="true"]').length, 4, "all visible MusicXML instruments start selected");
    assert.deepEqual(Array.from(app.document.querySelectorAll(".instrument-visibility-heading"), (heading) => heading.textContent), ["Visible instruments"], "empty hidden group is omitted");
    assert.deepEqual(Array.from(app.document.querySelectorAll(".source-count"), (element) => element.textContent), Array(4).fill("3 new staves"));
    await app.click('[data-action="select-all"]');
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
    assert.equal(app.document.querySelectorAll('[data-part][aria-checked="true"]').length, 0, "Merge starts empty for MusicXML");
    const create = app.find('[data-action="add-group"]');
    assert.equal(create.closest(".pane-body"), null, "group creation stays outside the scrolling list");
    assert.ok(create.closest(".source-pane"), "group creation stays in the sources pane");
    assert.equal(app.document.querySelector(".merge-source-row .source-count"), null, "Merge sources have no voice-count subline");
    for (const id of ["P1", "P2", "P3"]) await app.click(`[data-part="${id}"]`);
    await app.click('[data-action="add-group"]');
    assert.match(app.find(".merge-create").textContent!, /No instruments selected/);
    for (const id of ["P1", "P2", "P3"]) {
      const source = app.find(`[data-part="${id}"]`);
      assert.equal(source.getAttribute("aria-disabled"), "true", `${id} cannot join another group`);
      assert.equal(source.getAttribute("aria-checked"), "false", "group creation clears source selection");
      assert.equal(source.parentElement!.querySelector(".part-name + .source-status")?.textContent, "Grouped", "grouped indicator follows the instrument name inline");
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

test("all-hidden MusicXML omits the empty visible group and Select all retains source export order", async () => {
  const document = score();
  document.parts = document.parts.map((part) => ({ ...part, visible: false }));
  const { app, exported } = await arrangementApp(document);
  try {
    await app.click('[data-mode="split"]');
    assert.deepEqual(Array.from(app.document.querySelectorAll(".instrument-visibility-heading"), (heading) => heading.textContent), ["Hidden instruments"]);
    assert.equal(app.document.querySelectorAll('[data-part][aria-checked="true"]').length, 0);
    await app.click('[data-action="select-all"]');
    await app.click('[data-action="export"]');
    const split = exported.at(-1);
    assert.ok(split?.kind === "split");
    assert.deepEqual(split.part_ids, ["P1", "P2", "P3", "P4"]);
    await app.click('[data-action="back-to-modes"]');
    await app.click('[data-mode="merge"]');
    assert.deepEqual(Array.from(app.document.querySelectorAll(".instrument-visibility-heading"), (heading) => heading.textContent), ["Hidden instruments"]);
    assert.equal(app.document.querySelectorAll('[data-part][aria-checked="true"]').length, 0);
    await app.click('[data-action="select-all"]');
    await app.click('[data-action="add-group"]');
    await app.click('[data-action="export"]');
    assert.deepEqual(exported.at(-1), { kind: "merge", groups: [["P1", "P2", "P3", "P4"]], stems: [["up", "down", "up", "down"]] });
  } finally { await app.close(); }
});

function museScore(path: string): Inspection {
  return {
    ...score(), id: path, path, name: path.split("/").pop()!, format: "musescore",
    parts: score().parts.map((part, index) => ({
      ...part, visible: index % 2 === 0, staff_count: index === 3 ? 2 : 1, supported: index !== 3,
    })),
    practice_parts: [],
  };
}

test("MuseScore lists all sources, defaults by visibility, remembers each tool, and resets for a new score", async () => {
  let path = "/scores/first.mscz";
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return [path];
    if (command === "inspect_file") return { ...museScore(String(args.path)), parts: [] };
    if (command === "prepare_musicxml") return museScore(String(args.id));
    if (command === "practice_environment") return { executable: null, soundfont: null, version: null, presets: [], error: null };
    if (command === "release_document") return;
    throw new Error(`Unexpected command ${command}`);
  });
  const checked = () => Array.from(app.document.querySelectorAll('[data-part][aria-checked="true"]'), (part) => part.getAttribute("data-part"));
  const switchTool = async (tool: "split" | "merge") => {
    await app.click('[data-action="back-to-modes"]');
    await app.click(`[data-mode="${tool}"]`);
  };
  try {
    await app.menu("open-score");
    await app.click('[data-mode="split"]');
    assert.equal(app.document.querySelectorAll("[data-part]").length, 4);
    assert.deepEqual(checked(), ["P1", "P3"]);
    assert.deepEqual(Array.from(app.document.querySelectorAll(".instrument-visibility-heading"), (heading) => heading.textContent), ["Visible instruments", "Hidden instruments"]);
    assert.deepEqual(Array.from(app.find('[aria-label="Visible instruments"]').querySelectorAll("[data-part]"), (part) => part.getAttribute("data-part")), ["P1", "P3"]);
    assert.deepEqual(Array.from(app.find('[aria-label="Hidden instruments"]').querySelectorAll("[data-part]"), (part) => part.getAttribute("data-part")), ["P2", "P4"]);
    assert.doesNotMatch(app.find(".instruments").textContent!, /Hidden in (source|score)/);
    const disabled = app.find('[data-part="P4"]');
    assert.equal(disabled.getAttribute("aria-disabled"), "true");
    assert.equal(disabled.getAttribute("aria-checked"), "false");
    assert.match(app.find(`#${disabled.getAttribute("aria-describedby")}`).textContent!, /2 source staves.*unsupported.*Retained in export/);
    await app.click('[data-part="P4"]');
    assert.deepEqual(checked(), ["P1", "P3"]);
    await app.click('[data-action="select-all"]');
    assert.deepEqual(checked(), ["P1", "P3", "P2"]);
    await app.click('[data-action="select-all"]');
    assert.deepEqual(checked(), []);
    assert.equal(app.find<HTMLButtonElement>('[data-action="export"]').disabled, true);
    await switchTool("merge");
    assert.deepEqual(checked(), [], "Merge starts with no selection");
    assert.deepEqual(Array.from(app.document.querySelectorAll("[data-part]"), (part) => part.getAttribute("data-part")), ["P1", "P3", "P2", "P4"], "Merge groups visible before hidden, preserving source order within each group");
    assert.match(app.find('[data-part="P4"]').parentElement!.querySelector(".source-status")!.textContent!, /2 staves · Not supported/);
    const mergeUnsupported = app.find('[data-part="P4"]');
    assert.match(app.find(`#${mergeUnsupported.getAttribute("aria-describedby")}`).textContent!, /2 source staves.*unsupported.*Retained in export/);
    assert.equal(app.find<HTMLButtonElement>('[data-action="export"]').disabled, true, "a merge needs a group");
    assert.equal(app.find<HTMLButtonElement>('[data-action="add-group"]').disabled, true);
    await app.click('[data-part="P1"]');
    await app.click('[data-part="P3"]');
    await app.click('[data-action="add-group"]');
    assert.deepEqual(checked(), []);
    assert.equal(app.find('[data-part="P1"]').getAttribute("aria-disabled"), "true");
    await app.click('[data-action="select-all"]');
    assert.deepEqual(checked(), ["P2"], "select all excludes grouped and unsupported sources");
    await switchTool("split");
    assert.deepEqual(checked(), [], "Split keeps an explicit clear");
    await switchTool("merge");
    assert.deepEqual(checked(), ["P2"]);
    assert.equal(app.document.querySelectorAll(".merge-group").length, 1);
    await app.click('[data-action="remove-group"]');
    assert.deepEqual(checked(), ["P2"], "group removal preserves unrelated choices");
    assert.notEqual(app.find('[data-part="P1"]').getAttribute("aria-disabled"), "true");
    await app.click('[data-action="select-all"]');
    assert.deepEqual(checked(), ["P1", "P3", "P2"]);
    await app.click('[data-action="select-all"]');
    assert.deepEqual(checked(), []);
    path = "/scores/second.mscx";
    await app.menu("open-score");
    await app.click('[data-mode="split"]');
    assert.deepEqual(checked(), ["P1", "P3"], "new score resets defaults even when part IDs match");
  } finally { await app.close(); }
});

test("conversion warnings remain accessible through preparation, successful export, cancellation, and errors", async () => {
  const sourceWarning = 'Tempo text "Allegro" at measure 4, beat 1 needs review.';
  const preparedWarning = 'Rehearsal mark "B" at measure 12, beat 1 could not be preserved.';
  const exportWarning = "Playback tempo at measure 18, beat 2 could not be verified.";
  let result: "export" | "cancel" | "fail" = "export";
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return ["/scores/warnings.mscz"];
    if (command === "inspect_file") return { ...museScore(String(args.path)), parts: [], warnings: [sourceWarning] };
    if (command === "prepare_musicxml") return { ...museScore(String(args.id)), warnings: [preparedWarning] };
    if (command === "practice_environment") return { executable: null, soundfont: null, version: null, presets: [], error: null };
    if (command === "release_document") return;
    if (command === "export_document") {
      if (result === "fail") throw new Error("Cannot write score");
      return result === "cancel" ? null : { path: "/scores/result.mxl", warnings: [preparedWarning, exportWarning] };
    }
    throw new Error(`Unexpected command ${command}`);
  });
  try {
    await app.menu("open-score");
    await app.click('[data-mode="split"]');
    assert.deepEqual(Array.from(app.document.querySelectorAll(".conversion-warnings li"), (item) => item.textContent), [sourceWarning, preparedWarning]);
    assert.equal(app.find(".conversion-warnings").getAttribute("aria-live"), "polite");
    assert.equal(app.find(".conversion-warnings").getAttribute("tabindex"), "0", "scrolling warnings are keyboard accessible");
    await app.click('[data-action="export"]');
    assert.match(app.find(".status").textContent!, /Exported.*result.mxl.*Review warnings/s);
    assert.doesNotMatch(app.find(".status").textContent!, /affected passages are marked red/);
    const warnings = () => Array.from(app.document.querySelectorAll(".conversion-warnings li"), (item) => item.textContent);
    assert.deepEqual(warnings(), [sourceWarning, preparedWarning, exportWarning]);
    await app.choose("#split-mode", "Explicit voices");
    result = "cancel";
    await app.click('[data-action="export"]');
    assert.match(app.find(".status").textContent!, /cancelled/);
    assert.deepEqual(warnings(), [sourceWarning, preparedWarning, exportWarning]);
    result = "fail";
    await app.click('[data-action="export"]');
    assert.match(app.find('[role="alert"]').textContent!, /Cannot write score/);
    assert.deepEqual(warnings(), [sourceWarning, preparedWarning, exportWarning]);
    await app.click('[data-action="back-to-modes"]');
    await app.click('[data-mode="merge"]');
    assert.deepEqual(warnings(), [sourceWarning, preparedWarning, exportWarning]);
  } finally { await app.close(); }
});
