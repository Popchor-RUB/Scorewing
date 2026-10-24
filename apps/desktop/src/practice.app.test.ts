import assert from "node:assert/strict";
import test from "node:test";
import { act } from "react";
import type { PracticeConfig, PracticeOptions, PracticeResult } from "./model.ts";
import { mountTestApp } from "./test-support.ts";

test("practice UI configures sounds, validates exports, reports progress and retains partial results", async () => {
  let ready = false;
  let nextPath: string | null = "/scores/first.mscz";
  let finish: ((result: PracticeResult | null) => void) | undefined;
  let exportError: string | null = null;
  let cancelId = "";
  const requests: { id: string; options: PracticeOptions; config: PracticeConfig }[] = [];
  const configs: PracticeConfig[] = [];
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_file") return nextPath;
    if (command === "release_document") return;
    if (command === "inspect_file") {
      const path = String(args.path);
      return {
        id: path, path, name: path.split("/").pop(), error: null, parts: [], format: "musescore",
        practice_parts: [
          { key: "one", name: "Violin", visible: true, original_instruments: ["Violin"], existing_preset: { bank: 0, program: 40 } },
          { key: "two", name: "Violin", visible: false, original_instruments: ["Violin", "Viola"], existing_preset: null },
        ],
      };
    }
    if (command === "practice_environment") {
      configs.push(args.config as PracticeConfig);
      return {
        executable: ready ? "/bin/mscore" : null, soundfont: "/sounds/MS Basic.sf3", version: ready ? "4.7.3" : null,
        presets: [{ bank: 0, program: 40, name: "Violin" }, { bank: 0, program: 0, name: "Piano" }],
        error: ready ? null : "MuseScore 4.7.x was not found",
      };
    }
    if (command === "pick_practice_path") { ready = true; return "/chosen/mscore"; }
    if (command === "export_practice_tracks") {
      requests.push(args as unknown as (typeof requests)[number]);
      if (exportError) throw exportError;
      return await new Promise<PracticeResult | null>((resolve) => { finish = resolve; });
    }
    if (command === "cancel_practice_export") { cancelId = String(args.id); return; }
    throw new Error(`Unexpected command ${command}`);
  });
  try {
    await app.click('[data-action="open"]');
    assert.equal(app.find<HTMLInputElement>('input[type="radio"][value="practice"]').checked, true);
    assert.equal(app.find<HTMLInputElement>('input[type="radio"][value="split"]').disabled, true);
    assert.equal(app.document.querySelectorAll(".practice-part").length, 2);
    assert.match(app.find(".practice-parts").textContent!, /Hidden in score/);
    assert.equal(app.find<HTMLInputElement>('[data-practice-index="0"][data-practice-field="create_track"] input, input[data-practice-index="0"][data-practice-field="create_track"]').checked, true);
    assert.match(app.find(".practice-environment").textContent!, /was not found/);
    assert.match(app.find(".practice-environment").textContent!, /Keep MuseScore closed/);
    for (const id of ["foreground_sound-0", "background_sound-1", "background-reduction", "metronome-level"]) {
      const input = app.find<HTMLInputElement>(`#${id}`);
      assert.ok(input.matches("input"), `${id} identifies its input`);
      assert.ok(input.getAttribute("aria-labelledby")?.split(" ").every((label) => app.document.getElementById(label)), `${id} retains its accessible label`);
    }
    await app.click('[data-action="practice-export"]');
    assert.equal(requests.length, 0);
    await app.click('[data-action="practice-browse"][data-kind="executable"]');
    assert.equal(configs.at(-1)?.executable, "/chosen/mscore");
    await app.choose("#foreground_sound-0", "Piano");
    await app.click('[data-practice-index="0"][data-practice-field="include_in_background"]');
    await app.click('[data-practice-index="1"][data-practice-field="include_in_background"]');
    await app.click('[data-action="practice-export"]');
    assert.match(app.find('[role="alert"]').textContent!, /background sound for instrument 2/);
    await app.choose("#background_sound-1", "Violin");
    await app.click("#practice-metronome");
    await app.input("#metronome-level", "-12");
    await app.input("#background-reduction", "9");
    await app.click('[data-action="practice-export"]');
    assert.equal(requests.length, 1);
    assert.deepEqual(requests[0].options, {
      parts: [
        { key: "one", foreground_sound: { bank: 0, program: 0 }, background_sound: { bank: 0, program: 40 }, include_in_background: false, create_track: true },
        { key: "two", foreground_sound: null, background_sound: { bank: 0, program: 40 }, include_in_background: true, create_track: false },
      ], background_reduction_db: 9, metronome_enabled: true, metronome_db: -12,
    });
    assert.ok(app.find<HTMLInputElement>("#foreground_sound-0").disabled);
    assert.ok(app.find<HTMLButtonElement>('[data-action="open"]').disabled);
    assert.ok(app.find<HTMLButtonElement>('[data-action="close"]').disabled);
    await app.event("practice-progress", { id: "/scores/unrelated.mscz", stage: "Wrong score", completed: 1, total: 2 });
    assert.doesNotMatch(app.document.body.textContent!, /Wrong score/);
    await app.event("practice-progress", { id: "/scores/first.mscz", stage: "Rendering Violin", completed: 1, total: 2 });
    assert.match(app.find(".practice-progress").textContent!, /Rendering Violin.*1.*2/s);
    await app.click('[data-action="practice-cancel"]');
    assert.equal(cancelId, "/scores/first.mscz");
    await act(async () => {
      finish!({ directory: "/output/first", completed: [{ key: "one", name: "Violin", path: "/output/first/Violin.mp3" }], error: null, cancelled: true });
    });
    await app.settle();
    assert.match(app.find(".practice-results").textContent!, /Rendering cancelled/);
    assert.match(app.find(".practice-results").textContent!, /Violin.mp3/);
    assert.equal(app.find<HTMLButtonElement>('[data-action="practice-export"]').disabled, false);
    await app.event("practice-progress", { id: "/scores/first.mscz", stage: "Stale progress", completed: 2, total: 2 });
    assert.doesNotMatch(app.document.body.textContent!, /Stale progress/);
    await app.click('[data-action="practice-export"]');
    await act(async () => {
      finish!({ directory: "/output/second", completed: [{ key: "one", name: "Violin", path: "/output/second/Violin.mp3" }], error: "Renderer timed out", cancelled: false });
    });
    await app.settle();
    assert.match(app.find(".practice-results").textContent!, /Renderer timed out/);
    assert.match(app.find(".practice-results").textContent!, /1 completed track/);
    await app.click('[data-action="practice-export"]');
    await act(async () => { finish!(null); }); await app.settle();
    assert.match(app.find(".status").textContent!, /Export cancelled/);
    exportError = "Renderer launch failed";
    await app.click('[data-action="practice-export"]');
    assert.match(app.find('[role="alert"]').textContent!, /Renderer launch failed/);
    assert.equal(app.find<HTMLButtonElement>('[data-action="practice-export"]').disabled, false);
    await app.click('[data-action="close"]');
    nextPath = "/scores/second.mscx";
    await app.click('[data-action="open"]');
    assert.equal(app.find<HTMLInputElement>("#background-reduction").value, "6");
    assert.equal(app.find<HTMLInputElement>("#practice-metronome").checked, false);
    assert.equal(app.find<HTMLInputElement>("#foreground_sound-0").value, "Violin");
    assert.equal(configs.at(-1)?.executable, "/chosen/mscore", "renderer configuration survives score replacement");
    assert.equal(app.document.querySelector(".practice-results"), null, "score-specific results are cleared");
  } finally { await app.close(); }
});
