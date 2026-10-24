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
    for (const [selector, variant] of [[".card .checkbox", "checkbox--secondary"], [".card .combo-box .input", "input--secondary"], [".card .number-field", "number-field--secondary"], [".practice-path .input", "input--secondary"]]) {
      const controls = app.document.querySelectorAll(selector);
      assert.ok(controls.length, `Missing ${selector}`);
      for (const control of controls)
        assert.ok(control.classList.contains(variant), `${selector} uses the surface variant`);
    }
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
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
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
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.match(app.find(".practice-results").textContent!, /1 completed track/);
    await app.click('[data-action="practice-export"]');
    await act(async () => { finish!(null); }); await app.settle();
    assert.match(app.find(".status").textContent!, /Export cancelled/);
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
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

test("successful practice exports open a dismissible completion modal with a retryable folder action", async () => {
  let finish: (result: PracticeResult) => void;
  let openResolve: () => void;
  let openReject: (error: Error) => void;
  const opened: unknown[] = [];
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_file") return "/scores/practice.mscz";
    if (command === "release_document") return;
    if (command === "inspect_file") return {
      id: "practice", name: "practice.mscz", path: "/scores/practice.mscz", format: "musescore", parts: [], error: null,
      practice_parts: [{ key: "one", name: "Violin", visible: true, original_instruments: [], existing_preset: { bank: 0, program: 40 } }],
    };
    if (command === "practice_environment") return {
      executable: "/bin/mscore", soundfont: "/sounds/MS Basic.sf3", version: "4.7.5", error: null,
      presets: [{ bank: 0, program: 40, name: "Violin" }],
    };
    if (command === "export_practice_tracks") return new Promise<PracticeResult>((resolve) => { finish = resolve; });
    if (command === "open_practice_export_folder") {
      opened.push(args.directory);
      return new Promise<void>((resolve, reject) => { openResolve = resolve; openReject = reject; });
    }
    throw new Error(`Unexpected command ${command}`);
  });
  const result = (directory: string): PracticeResult => ({
    directory, completed: [{ key: "one", name: "Violin", path: `${directory}/Violin.mp3` }], error: null, cancelled: false,
  });
  const complete = async (directory: string) => {
    await app.click('[data-action="practice-export"]');
    assert.equal(app.document.querySelector('[role="dialog"]'), null, "rendering has no completion modal");
    await act(async () => finish(result(directory)));
    await app.settle();
  };
  try {
    await app.click('[data-action="open"]');
    const directory = "/output/Practice tracks – 音楽 (2)";
    await complete(directory);
    const dialog = app.find('[role="dialog"]');
    assert.match(dialog.textContent!, /Practice tracks created/);
    assert.match(dialog.textContent!, /1 completed track saved in/);
    assert.equal(app.find(".practice-export-path").textContent, directory);
    assert.equal(app.find(".practice-export-hero").getAttribute("aria-hidden"), "true");
    assert.ok(app.document.activeElement?.closest('[role="dialog"]'), "focus moves into the modal");
    assert.equal(opened.length, 0, "the file browser opens only on request");
    await app.click('[data-action="practice-open-folder"]');
    assert.deepEqual(opened, [directory]);
    assert.ok(app.find<HTMLButtonElement>('[data-action="practice-open-folder"]').disabled);
    await app.click('[data-action="practice-open-folder"]');
    assert.equal(opened.length, 1, "duplicate clicks do not start another request");
    await act(async () => openReject(new Error("File browser unavailable")));
    await app.settle();
    assert.match(app.find('[role="dialog"] [role="alert"]').textContent!, /File browser unavailable/);
    assert.equal(app.find(".practice-export-path").textContent, directory);
    assert.equal(app.find<HTMLButtonElement>('[data-action="practice-open-folder"]').disabled, false);
    await app.click('[data-action="practice-open-folder"]');
    await act(async () => openResolve());
    await app.settle();
    assert.equal(app.document.querySelector('[role="dialog"] [role="alert"]'), null);
    assert.ok(app.find('[role="dialog"]'), "opening a folder keeps the modal visible");
    await app.click('[data-action="practice-export-done"]');
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.equal(app.document.activeElement?.getAttribute("data-action"), "practice-export", "focus returns to the export action");
    assert.ok(app.find(".practice-results"), "dismissing keeps the export results");
    await app.click("#practice-metronome");
    await app.event("practice-progress", { id: "practice", completed: 1, total: 1, stage: "Late event" });
    assert.equal(app.document.querySelector('[role="dialog"]'), null, "rerenders do not reopen a dismissed modal");

    await complete("C:\\Users\\Music\\Practice tracks (3)");
    assert.equal(app.find(".practice-export-path").textContent, "C:\\Users\\Music\\Practice tracks (3)");
    await act(async () => { app.document.activeElement?.dispatchEvent(new app.browser.KeyboardEvent("keydown", { key: "Escape", bubbles: true })); });
    await app.settle();
    assert.equal(app.document.querySelector('[role="dialog"]'), null, "Escape dismisses the modal");
    await app.click('[data-action="close"]');
    await app.click('[data-action="open"]');
    assert.equal(app.document.querySelector('[role="dialog"]'), null, "a new score has no old completion state");

    await complete("/Users/Music/Practice tracks");
    await app.click('[aria-label="Close practice track completion"]');
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    await app.click('[data-action="practice-export"]');
    await app.remount();
    await act(async () => finish(result("/output/disposed")));
    await app.settle();
    assert.equal(app.document.querySelector('[role="dialog"]'), null, "a disposed session cannot show completion");
    assert.doesNotMatch(app.document.body.textContent!, /disposed/);
  } finally { await app.close(); }
});
