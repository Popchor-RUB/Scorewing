import assert from "node:assert/strict";
import test from "node:test";
import { act } from "react";
import type { HTMLInputElement as TestInputElement } from "happy-dom";
import type { PracticeConfig, PracticeEnvironment, PracticeOptions, PracticeResult } from "./model.ts";
import { mountTestApp } from "./test-support.ts";
import { PRACTICE_DEFAULTS_KEY } from "./practice-defaults.ts";

test("practice conflicts require a choice, preserve cancellation, and disable unsafe clearing", async () => {
  let finish: (result: PracticeResult | null) => void;
  let failReply = false;
  const replies: Record<string, unknown>[] = [];
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_files") return ["/scores/practice.mscz"];
    if (command === "release_document") return;
    if (command === "inspect_file") return {
      id: "practice", name: "practice.mscz", path: "/scores/practice.mscz", format: "musescore", parts: [], error: null,
      practice_parts: [{ key: "one", name: "Piano", visible: true, original_instruments: [], existing_preset: { bank: 0, program: 0 } }],
    };
    if (command === "practice_environment") return {
      executable: "/bin/mscore", soundfont: "/sounds/MS Basic.sf3", version: "4.7.5", error: null,
      presets: [{ bank: 0, program: 0, name: "Piano" }],
    };
    if (command === "export_practice_tracks") return new Promise<PracticeResult | null>((resolve) => { finish = resolve; });
    if (command === "resolve_practice_export") {
      if (failReply) { failReply = false; throw new Error("Reply failed"); }
      replies.push(args);
      if (args.choice === "cancel") finish(null);
      return;
    }
    if (command === "cancel_practice_export") { finish(null); return; }
    throw new Error(`Unexpected command ${command}`);
  });
  const conflict = { id: "practice", token: "first", directory: "/output/rehearsal", conflicts: 6, total: 8, clear_error: null as string | null };
  try {
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    await app.click('[data-action="practice-export"]');
    await app.event("practice-export-conflict", { ...conflict, id: "other-score" });
    assert.match(app.find('[role="dialog"]').textContent!, /Exporting practice tracks/);
    await app.event("practice-export-conflict", conflict);
    assert.match(app.find('[role="dialog"]').textContent!, /6 of the 8.*\/output\/rehearsal/s);
    assert.equal(app.document.activeElement, app.find('[data-action="practice-conflict-cancel"]'));
    assert.equal(app.find('[data-action="practice-overwrite-choice"]').getAttribute("aria-checked"), "true");
    assert.equal(app.find('[data-action="practice-conflict-confirm"]').textContent, "Overwrite & export");
    assert.equal(replies.length, 0);
    failReply = true;
    await app.click('[data-action="practice-conflict-confirm"]');
    assert.match(app.find('[role="dialog"] [role="alert"]').textContent!, /Reply failed/);
    await app.click('[data-action="practice-conflict-confirm"]');
    assert.deepEqual(replies.at(-1), { id: "practice", token: "first", choice: "overwrite" });
    await act(async () => finish(null)); await app.settle();

    await app.click('[data-action="practice-export"]');
    await app.event("practice-export-conflict", { ...conflict, token: "second" });
    await app.click('[data-action="practice-clear-choice"]');
    assert.equal(app.find('[data-action="practice-conflict-confirm"]').textContent, "Clear folder & export");
    assert.equal(app.find('[data-action="practice-conflict-confirm"]').getAttribute("data-variant"), "destructive");
    assert.match(app.find('[role="dialog"]').textContent!, /including unrelated files and subfolders/);
    await app.click('[data-action="practice-conflict-confirm"]');
    assert.deepEqual(replies.at(-1), { id: "practice", token: "second", choice: "clear" });
    await act(async () => finish(null)); await app.settle();

    await app.click('[data-action="practice-export"]');
    await app.event("practice-export-conflict", { ...conflict, token: "third", clear_error: "Cannot clear a source score folder." });
    assert.equal(app.find('[data-action="practice-clear-choice"]').getAttribute("aria-disabled"), "true");
    await act(async () => { app.document.activeElement?.dispatchEvent(new app.browser.KeyboardEvent("keydown", { key: "Escape", bubbles: true })); });
    await app.settle();
    assert.deepEqual(replies.at(-1), { id: "practice", token: "third", choice: "cancel" });
    assert.equal(app.document.querySelector('[role="dialog"]'), null);

    assert.equal(app.document.activeElement, app.find('[data-action="practice-export"]'), "cancel restores focus to the export action");

    await app.click('[data-action="practice-export"]');
    await app.event("practice-export-conflict", { ...conflict, token: "fourth" });
    await app.click('[data-action="practice-conflict-cancel"]');
    assert.deepEqual(replies.at(-1), { id: "practice", token: "fourth", choice: "cancel" });
    assert.match(app.find(".status").textContent!, /Export cancelled/);
  } finally { await app.close(); }
});

test("practice ranges support section-only exports, preserve choices, and count every mix", async () => {
  const requests: PracticeOptions[] = [];
  let finish: ((result: PracticeResult | null) => void) | undefined;
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_files") return ["/scores/sections.mscz"];
    if (command === "release_document") return;
    if (command === "inspect_file") return {
      id: "sections", path: args.path, name: "sections.mscz", error: null, parts: [], format: "musescore",
      practice_parts: [{ key: "one", name: "Piano", visible: true, original_instruments: ["piano"], existing_preset: { bank: 0, program: 0 } }],
      practice_sections: [
        { key: "measure-1", name: "Intro", start_measure: 1, end_measure: 2 },
        { key: "measure-3", name: "A", start_measure: 3, end_measure: 5 },
        { key: "measure-6", name: "A", start_measure: 6, end_measure: 8 },
      ],
    };
    if (command === "practice_environment") return {
      executable: "/bin/mscore", soundfont: "/sounds/MS Basic.sf3", version: "4.7.5", error: null,
      presets: [{ bank: 0, program: 0, name: "Piano" }],
    };
    if (command === "export_practice_tracks") {
      requests.push(args.options as PracticeOptions);
      return await new Promise<PracticeResult | null>((resolve) => { finish = resolve; });
    }
    throw new Error(`Unexpected command ${command}`);
  });
  try {
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    assert.equal(app.find<HTMLInputElement>('[data-action="practice-whole-song"] input').checked, true);
    assert.equal(app.document.querySelectorAll('[data-section-key]').length, 3);
    for (const input of app.document.querySelectorAll<TestInputElement>('[aria-label="Practice ranges"] input')) assert.equal(input.checked, true);
    assert.equal(app.find('[data-action="practice-select-all"]').textContent, "Select all");
    assert.equal(app.find('[data-action="practice-clear-all"]').textContent, "Clear");
    assert.equal(app.document.querySelector("#practice-hidden-instruments"), null, "empty visibility groups are omitted");
    assert.match(app.find('.workspace-footer').textContent!, /1 instruments · 4 ranges/);
    assert.match(app.find('[data-section-key="measure-3"]').textContent!, /A.*Measures 3–5/s);
    assert.match(app.find('[aria-label="Practice ranges"]').textContent!, /first playback arrival.*follow repeats and jumps.*up to two played measures after leaving the displayed range/s);
    await app.click('[data-action="practice-clear-all"]');
    for (const input of app.document.querySelectorAll<TestInputElement>('[aria-label="Practice ranges"] input')) assert.equal(input.checked, false);
    await app.click('[data-action="practice-select-all"]');
    for (const input of app.document.querySelectorAll<TestInputElement>('[aria-label="Practice ranges"] input')) assert.equal(input.checked, true);
    await app.click('[data-section-key="measure-3"]');
    await app.click('[data-action="practice-select-all"]');
    assert.equal(app.find<HTMLInputElement>('[data-section-key="measure-3"] input').checked, true);
    await app.click('[data-action="practice-clear-all"]');
    await app.click('[data-action="practice-export"]');
    assert.equal(requests.length, 0);
    assert.match(app.document.body.textContent!, /Choose the whole song or at least one rehearsal section/);
    await app.click('[data-section-key="measure-3"]');
    await app.click('[data-section-key="measure-6"]');
    await app.click('[data-action="back-to-modes"]');
    await app.click('[data-mode="practice"]');
    assert.equal(app.find<HTMLInputElement>('[data-section-key="measure-3"] input').checked, true);
    await app.click('[data-action="practice-export"]');
    assert.equal(requests.length, 1);
    assert.equal(requests[0].include_whole_song, false);
    assert.deepEqual(requests[0].section_keys, ["measure-3", "measure-6"]);
    assert.match(app.find(".practice-progress").textContent!, /0 \/ 2 tracks completed/);
    for (const input of app.document.querySelectorAll('[aria-label="Practice ranges"] input')) assert.ok(input.hasAttribute("disabled"));
    assert.ok(app.find('[data-action="practice-select-all"]').hasAttribute("disabled"));
    assert.ok(app.find('[data-action="practice-clear-all"]').hasAttribute("disabled"));
    await act(async () => finish!(null));
    await app.settle();
    await app.menu("close-score");
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    assert.equal(app.find<HTMLInputElement>('[data-action="practice-whole-song"] input').checked, true);
    for (const input of app.document.querySelectorAll<TestInputElement>('[aria-label="Practice ranges"] input')) assert.equal(input.checked, true, "reopened scores start with every range selected");
  } finally { await app.close(); }
});

test("practice visibility groups preserve source indices and independent sound roles", async () => {
  const requests: PracticeOptions[] = [];
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_files") return ["/scores/grouped.mscz"];
    if (command === "release_document") return;
    if (command === "inspect_file") return {
      id: "grouped", name: "grouped.mscz", path: args.path, format: "musescore", parts: [], error: null,
      practice_parts: [false, true, false, true].map((visible, index) => ({
        key: `part-${index}`, name: `Source ${index}`, visible, original_instruments: [], existing_preset: { bank: 0, program: 40 },
      })),
    };
    if (command === "practice_environment") return {
      executable: "/bin/mscore", soundfont: "/sounds/MS Basic.sf3", version: "4.7.5", error: null,
      presets: [{ bank: 0, program: 40, name: "Violin" }, { bank: 0, program: 0, name: "Piano" }],
    };
    if (command === "export_practice_tracks") { requests.push(args.options as PracticeOptions); return null; }
    throw new Error(`Unexpected command ${command}`);
  });
  const picker = (role: "foreground" | "background", index: number) => app.find<HTMLInputElement>(`#${role}_sound-${index}`);
  const choice = (field: "create_track" | "include_in_background", index: number) => `[data-practice-index="${index}"][data-practice-field="${field}"]`;
  const defaultButton = (role: "foreground" | "background", index: number) => app.find<HTMLButtonElement>(
    `[aria-labelledby="practice-part-${index}"] [aria-label="Set default ${role} instrument"]`);
  try {
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    assert.deepEqual([...app.document.querySelectorAll(".practice-visibility-group > h3")].map((heading) => heading.textContent), ["Visible instruments", "Hidden instruments"]);
    assert.deepEqual([...app.document.querySelectorAll(".practice-part > h4")].map((heading) => heading.id), ["practice-part-1", "practice-part-3", "practice-part-0", "practice-part-2"]);
    assert.doesNotMatch(app.find(".practice-parts").textContent!, /Hidden in score/);
    assert.match(app.find(".practice-instruments-explanation").textContent!, /one MP3 per selected range.*Background instruments accompany/s);
    for (const index of [0, 1, 2, 3]) {
      const visible = index % 2 === 1;
      assert.equal(app.find(`#practice-part-${index}`).textContent, `Source ${index}`);
      for (const [role, field, label] of [
        ["foreground", "create_track", "Create practice track"],
        ["background", "include_in_background", "Include in background"],
      ] as const) {
        assert.equal(app.find<HTMLInputElement>(`${choice(field, index)} input`).checked, visible);
        assert.match(app.find(choice(field, index)).textContent!, new RegExp(`${label}.*Source ${index}`));
        assert.equal(picker(role, index).disabled, !visible);
        assert.equal(picker(role, index).value, "Violin", "saved sounds stay visible for unchecked roles");
        assert.equal(defaultButton(role, index).disabled, !visible);
        assert.equal(picker(role, index).closest(".practice-sound-row"), app.find(choice(field, index)).parentElement, "checkbox and corresponding sound share a row");
      }
    }
    await app.choose("#foreground_sound-3", "Piano");
    await app.click(choice("create_track", 3));
    assert.ok(picker("foreground", 3).disabled);
    assert.ok(defaultButton("foreground", 3).disabled);
    assert.equal(picker("foreground", 3).value, "Piano");
    assert.equal(picker("background", 3).disabled, false, "foreground membership does not disable background");
    await app.click(choice("create_track", 3));
    await app.choose("#background_sound-3", "Piano");
    await app.click(choice("include_in_background", 3));
    assert.ok(picker("background", 3).disabled);
    assert.ok(defaultButton("background", 3).disabled);
    assert.equal(picker("background", 3).value, "Piano");
    assert.equal(picker("foreground", 3).disabled, false, "background membership does not disable foreground");
    await app.click(choice("create_track", 2));
    await app.click('[data-action="back-to-modes"]');
    await app.click('[data-mode="practice"]');
    assert.equal(picker("foreground", 3).value, "Piano");
    assert.equal(picker("background", 3).value, "Piano");
    assert.ok(picker("background", 3).disabled, "manual role choices survive tool navigation");
    assert.equal(picker("foreground", 2).disabled, false);
    assert.ok(picker("background", 2).disabled);
    await app.click('[data-action="practice-export"]');
    assert.equal(requests.length, 1);
    assert.deepEqual(requests[0].parts.map((part) => [part.key, part.create_track, part.include_in_background]), [
      ["part-0", false, false], ["part-1", true, true], ["part-2", true, false], ["part-3", true, false],
    ], "grouping never reorders source options or applies edits to a neighboring source");
    assert.deepEqual(requests[0].parts[3].foreground_sound, { bank: 0, program: 0 });
    assert.deepEqual(requests[0].parts[3].background_sound, { bank: 0, program: 0 });
    assert.deepEqual(requests[0].parts[1].foreground_sound, { bank: 0, program: 40 });
  } finally { await app.close(); }
});

test("practice UI configures sounds, validates exports, reports progress and retains partial results", async () => {
  let ready = false;
  let nextPath: string | null = "/scores/first.mscz";
  let finish: ((result: PracticeResult | null) => void) | undefined;
  let exportError: string | null = null;
  let cancelId = "";
  let folderAttempts = 0;
  const requests: { id: string; options: PracticeOptions; config: PracticeConfig }[] = [];
  const configs: PracticeConfig[] = [];
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_files") return nextPath ? [nextPath] : null;
    if (command === "release_document") return;
    if (command === "inspect_file") {
      const path = String(args.path);
      return {
        id: path, path, name: path.split("/").pop(), error: null, parts: [], format: "musescore",
        practice_parts: [
          { key: "one", name: "Alt 2", visible: true, original_instruments: ["alto"], existing_preset: { bank: 0, program: 40 } },
          { key: "two", name: "Alt 2", visible: false, original_instruments: ["alto", "voice"], existing_preset: null },
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
    if (command === "open_practice_export_folder") {
      assert.equal(args.directory, "/output/second");
      if (++folderAttempts === 1) throw new Error("File browser unavailable");
      return;
    }
    throw new Error(`Unexpected command ${command}`);
  });
  try {
    await app.menu("open-score");
    assert.equal(app.document.querySelector(".practice-part"), null, "loading stops at mode selection");
    assert.equal(app.find<HTMLButtonElement>('[data-mode="split"]').disabled, false);
    await app.click('[data-mode="practice"]');
    assert.equal(app.document.querySelectorAll(".practice-part").length, 2);
    assert.equal(app.document.querySelectorAll('[data-section-key]').length, 0);
    await app.click('[data-action="practice-clear-all"]');
    assert.equal(app.find<HTMLInputElement>('[data-action="practice-whole-song"] input').checked, false);
    await app.click('[data-action="practice-select-all"]');
    assert.equal(app.find<HTMLInputElement>('[data-action="practice-whole-song"] input').checked, true);
    for (const index of [0, 1]) {
      assert.equal(app.find(`#practice-part-${index}`).textContent, "Alt 2", "instrument titles show only the score's long name");
      assert.equal(app.find(`.practice-part[aria-labelledby="practice-part-${index}"]`).getAttribute("role"), "group");
    }
    assert.doesNotMatch(app.find(".practice-parts").textContent!, /Instrument \d|alto|voice/);
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.equal(app.document.querySelector(".practice-path"), null);
    assert.match(app.find(".practice-environment [role='alert']").textContent!, /⚠.*Open settings/);
    assert.ok(app.find(".practice-environment [role='alert']").classList.contains("text-danger"));
    assert.equal(app.find('[aria-label="Practice instruments"]').getAttribute("aria-disabled"), "true");
    for (const input of app.document.querySelectorAll('[aria-label="Practice instruments"] input'))
      assert.ok(input.hasAttribute("disabled"));
    await app.click('[data-action="practice-export"]');
    assert.equal(requests.length, 0);
    await app.click('[data-action="practice-settings"]');
    assert.match(app.find('[role="dialog"]').textContent!, /MuseScore settings/);
    assert.doesNotMatch(app.find(".practice-parts").textContent!, /Hidden in score/);
    assert.equal(app.find("#practice-visible-instruments").textContent, "Visible instruments");
    assert.equal(app.find("#practice-hidden-instruments").textContent, "Hidden instruments");
    assert.equal(app.find<HTMLInputElement>('[data-practice-index="0"][data-practice-field="create_track"] input, input[data-practice-index="0"][data-practice-field="create_track"]').checked, true);
    assert.match(app.find('[role="dialog"]').textContent!, /was not found/);
    assert.match(app.find('[role="dialog"]').textContent!, /Keep MuseScore closed/);
    assert.ok(app.find<HTMLInputElement>("#foreground_sound-1").disabled);
    assert.ok(app.find<HTMLInputElement>("#background_sound-1").disabled);
    assert.equal(app.document.querySelectorAll(".sound-picker").length, 4, "inactive instruments retain disabled sound pickers");
    assert.deepEqual([...app.document.querySelectorAll('.practice-part .sound-picker input[role="combobox"]')].map((input) => input.id),
      ["foreground_sound-0", "background_sound-0", "foreground_sound-1", "background_sound-1"], "foreground and background sounds remain separate controls");
    for (const [id, label] of [["background_sound-0", "Background Instrument"], ["foreground_sound-0", "Foreground Instrument"]]) {
      const labels = app.find(`#${id}`).getAttribute("aria-labelledby")!.split(" ");
      assert.equal(labels.map((id) => app.document.getElementById(id)!.textContent).join(" "), `${label} · Alt 2, instrument 1`);
    }
    assert.equal(app.document.querySelector('.sound-picker [aria-label^="Clear "]'), null);
    for (const id of ["foreground_sound-0", "background_sound-0", "background-reduction", "metronome-level"]) {
      const input = app.find<HTMLInputElement>(`#${id}`);
      assert.ok(input.matches("input"), `${id} identifies its input`);
      assert.ok(input.getAttribute("aria-labelledby")?.split(" ").every((label) => app.document.getElementById(label)), `${id} retains its accessible label`);
    }
    await app.click('[data-action="practice-browse"][data-kind="executable"]');
    assert.equal(configs.at(-1)?.executable, "/chosen/mscore");
    assert.match(app.find(".practice-environment").textContent!, /Found and using MuseScore Version 4.7.3/);
    await app.click('[aria-label="Close MuseScore settings"]');
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.equal(app.document.activeElement, app.find('[data-action="practice-settings"]'));
    assert.equal(app.find('[aria-label="Practice instruments"]').getAttribute("aria-disabled"), "false");
    assert.equal(app.find<HTMLInputElement>("#foreground_sound-0").disabled, false);
    await app.choose("#foreground_sound-0", "Piano");
    await app.click('[data-practice-index="0"][data-practice-field="include_in_background"]');
    assert.ok(app.find<HTMLInputElement>("#background_sound-0").disabled);
    assert.equal(app.find<HTMLInputElement>("#foreground_sound-0").value, "Piano");
    await app.click('[data-practice-index="0"][data-practice-field="create_track"]');
    assert.ok(app.find<HTMLInputElement>("#foreground_sound-0").disabled);
    assert.equal(app.document.querySelectorAll(".sound-picker").length, 4);
    await app.click('[data-practice-index="0"][data-practice-field="include_in_background"]');
    assert.ok(app.find<HTMLInputElement>("#foreground_sound-0").disabled);
    assert.equal(app.find<HTMLInputElement>("#background_sound-0").disabled, false);
    assert.equal(app.find<HTMLInputElement>("#background_sound-0").value, "Violin", "disabling preserves the background choice");
    await app.click('[data-practice-index="0"][data-practice-field="create_track"]');
    assert.equal(app.find<HTMLInputElement>("#foreground_sound-0").value, "Piano", "disabling preserves the foreground choice");
    await app.click('[data-practice-index="0"][data-practice-field="include_in_background"]');
    await app.click('[data-practice-index="1"][data-practice-field="include_in_background"]');
    assert.ok(app.find<HTMLInputElement>("#foreground_sound-1").disabled);
    assert.equal(app.find<HTMLInputElement>("#background_sound-1").disabled, false);
    await app.click('[data-action="practice-export"]');
    assert.match(app.find('[role="alert"]').textContent!, /background sound for instrument 2/);
    await app.choose("#background_sound-1", "Violin");
    await app.click("#practice-metronome");
    await app.input("#metronome-level", "-12");
    await app.input("#background-reduction", "9");
    await app.click('[data-action="back-to-modes"]');
    assert.equal(app.document.activeElement, app.find('[data-mode="practice"]'));
    await app.click('[data-mode="practice"]');
    assert.equal(app.find<HTMLInputElement>("#background-reduction").value, "9", "Back preserves practice levels");
    assert.equal(app.find<HTMLInputElement>("#foreground_sound-0").value, "Piano", "Back preserves practice sounds");
    await app.click('[data-action="practice-export"]');
    assert.equal(requests.length, 1);
    assert.deepEqual(requests[0].options, {
      include_whole_song: true, section_keys: [],
      parts: [
        { key: "one", foreground_sound: { bank: 0, program: 0 }, background_sound: { bank: 0, program: 40 }, include_in_background: false, create_track: true },
        { key: "two", foreground_sound: null, background_sound: { bank: 0, program: 40 }, include_in_background: true, create_track: false },
      ], background_reduction_db: 9, metronome_enabled: true, metronome_db: -12,
    });
    assert.ok(app.find<HTMLInputElement>("#foreground_sound-0").disabled);
    assert.ok(!app.fileMenu.openEnabled);
    assert.ok(!app.fileMenu.closeEnabled);
    assert.ok(app.find<HTMLButtonElement>('[data-action="back-to-modes"]').disabled);
    await app.event("practice-progress", { id: "/scores/unrelated.mscz", stage: "Wrong score", completed: 1, running: 1, total: 2 });
    assert.doesNotMatch(app.document.body.textContent!, /Wrong score/);
    await app.event("practice-progress", { id: "/scores/first.mscz", stage: "Rendering Violin", completed: 1, running: 1, total: 2 });
    assert.match(app.find(".practice-progress").textContent!, /Rendering Violin.*1.*2/s);
    assert.ok(app.find(".practice-progress").closest('[role="dialog"]'));
    assert.match(app.find(".practice-progress").textContent!, /1 export currently running/);
    await app.click('[data-action="practice-cancel"]');
    assert.equal(cancelId, "/scores/first.mscz");
    assert.match(app.find('[role="dialog"]').textContent!, /Cancelling…/);
    assert.ok(app.find<HTMLButtonElement>('[data-action="practice-cancel"]').disabled);
    await act(async () => {
      finish!({ directory: "/output/first", completed: [{ key: "one", name: "Violin", path: "/output/first/Violin.mp3" }], error: null, cancelled: true });
    });
    await app.settle();
    assert.match(app.find(".practice-results").textContent!, /Rendering cancelled/);
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.match(app.find(".practice-results").textContent!, /Violin.mp3/);
    assert.equal(app.find<HTMLButtonElement>('[data-action="practice-export"]').disabled, false);
    await app.event("practice-progress", { id: "/scores/first.mscz", stage: "Stale progress", completed: 2, running: 0, total: 2 });
    assert.doesNotMatch(app.document.body.textContent!, /Stale progress/);
    await app.click('[data-action="practice-export"]');
    await act(async () => {
      finish!({ directory: "/output/second", completed: [{ key: "one", name: "Violin", path: "/output/second/Violin.mp3" }], error: "Renderer timed out", cancelled: false });
    });
    await app.settle();
    assert.match(app.find(".practice-results").textContent!, /Renderer timed out/);
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.match(app.find(".practice-results").textContent!, /1 completed track/);
    await app.click('[data-action="practice-partial-open-folder"]');
    assert.match(app.find(".practice-results").textContent!, /File browser unavailable/);
    assert.match(app.find(".practice-results").textContent!, /Violin.mp3/);
    await app.click('[data-action="practice-partial-open-folder"]');
    assert.equal(folderAttempts, 2);
    assert.doesNotMatch(app.find(".practice-results").textContent!, /File browser unavailable/);
    await app.click('[data-action="practice-export"]');
    await act(async () => { finish!(null); }); await app.settle();
    assert.match(app.find(".status").textContent!, /Export cancelled/);
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    exportError = "Renderer launch failed";
    await app.click('[data-action="practice-export"]');
    assert.match(app.find('[role="alert"]').textContent!, /Renderer launch failed/);
    assert.equal(app.find<HTMLButtonElement>('[data-action="practice-export"]').disabled, false);
    await app.menu("close-score");
    nextPath = "/scores/second.mscx";
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    assert.equal(app.find<HTMLInputElement>("#background-reduction").value, "6", "unsaved levels reset on the next score");
    assert.equal(app.find<HTMLInputElement>("#practice-metronome").checked, true);
    assert.equal(app.find<HTMLInputElement>("#metronome-level").value, "-6");
    assert.equal(app.find<HTMLInputElement>("#foreground_sound-0").value, "Violin");
    assert.equal(configs.at(-1)?.executable, "/chosen/mscore", "renderer configuration survives score replacement");
    assert.equal(app.document.querySelector(".practice-results"), null, "score-specific results are cleared");
  } finally { await app.close(); }
});

test("successful practice exports open a dismissible completion modal with a retryable folder action", async (t) => {
  let finish: (result: PracticeResult) => void;
  let openResolve: () => void;
  let openReject: (error: Error) => void;
  const opened: unknown[] = [];
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_files") return ["/scores/practice.mscz"];
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
    directory, completed: [{ key: "one", name: "Violin", path: `${directory}/00 - Whole song/01 - Violin.mp3` }], error: null, cancelled: false,
  });
  let now = 0;
  let tick: (() => void) | undefined;
  const timerWindow = app.browser as unknown as Window;
  const setInterval = timerWindow.setInterval.bind(timerWindow);
  const clearInterval = timerWindow.clearInterval.bind(timerWindow);
  t.mock.method(performance, "now", () => now);
  t.mock.method(timerWindow, "setInterval", (callback: TimerHandler, delay?: number, ...args: unknown[]) => {
    if (delay !== 100) return setInterval(callback, delay, ...args);
    assert.equal(typeof callback, "function");
    tick = callback as () => void;
    return 123456;
  });
  t.mock.method(timerWindow, "clearInterval", (id?: number) => {
    if (id === 123456) tick = undefined;
    else clearInterval(id);
  });
  const advance = async (ms: number) => {
    now += ms;
    await act(async () => tick?.());
    await app.settle();
  };
  const percentage = () => Number(app.find('[role="progressbar"]').getAttribute("aria-valuenow"));
  const complete = async (directory: string) => {
    await app.click('[data-action="practice-export"]');
    assert.match(app.find('[role="dialog"]').textContent!, /Exporting practice tracks.*Choose a folder.*0 exports currently running/s);
    assert.equal(percentage(), 0, "each export starts at zero");
    await act(async () => { app.document.activeElement?.dispatchEvent(new app.browser.KeyboardEvent("keydown", { key: "Escape", bubbles: true })); });
    await app.settle();
    assert.ok(app.find('[role="dialog"]'), "Escape cannot dismiss a running export");
    await app.event("practice-progress", { id: "practice", completed: 0, running: 1, total: 1, stage: "Rendering Violin" });
    await advance(1000);
    assert.equal(percentage(), 10, "progress moves before a track completes");
    await advance(20_000);
    assert.equal(percentage(), 99, "estimated progress waits for successful completion");
    await app.event("practice-progress", { id: "practice", completed: 1, running: 0, total: 1, stage: "Practice tracks complete" });
    assert.equal(percentage(), 99);
    assert.equal(tick, undefined, "no active jobs leaves no timer");
    await act(async () => finish(result(directory)));
    await app.settle();
    assert.equal(app.document.querySelector('[role="progressbar"]'), null);
    assert.equal(app.document.querySelector(".practice-results"), null, "successful exports show results only in the modal");
    assert.equal(app.find(".status").textContent, "Practice tracks created.");
  };
  try {
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    const directory = "/output/Practice tracks – 音楽 (2)";
    await complete(directory);
    const dialog = app.find('[role="dialog"]');
    assert.match(dialog.textContent!, /Practice tracks created/);
    assert.match(dialog.textContent!, /1 completed track saved in/);
    assert.equal(app.find(".practice-export-path").textContent, directory);
    assert.equal(app.find('[aria-label="Exported section folders"]').textContent, "00 - Whole song/");
    assert.match(dialog.textContent!, /Section folders are saved directly here/);
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
    assert.equal(app.document.querySelector(".practice-results"), null, "dismissing does not show export paths in the main window");
    assert.ok(!app.document.body.textContent!.includes(directory), "the output path disappears with the modal");
    await app.click("#practice-metronome");
    await app.event("practice-progress", { id: "practice", completed: 1, running: 0, total: 1, stage: "Late event" });
    assert.equal(app.document.querySelector('[role="dialog"]'), null, "rerenders do not reopen a dismissed modal");

    await complete("C:\\Users\\Music\\Practice tracks (3)");
    assert.equal(app.find(".practice-export-path").textContent, "C:\\Users\\Music\\Practice tracks (3)");
    await act(async () => { app.document.activeElement?.dispatchEvent(new app.browser.KeyboardEvent("keydown", { key: "Escape", bubbles: true })); });
    await app.settle();
    assert.equal(app.document.querySelector('[role="dialog"]'), null, "Escape dismisses the modal");
    await app.menu("close-score");
    await app.menu("open-score");
    assert.equal(app.document.querySelector('[role="dialog"]'), null, "a new score has no old completion state");
    await app.click('[data-mode="practice"]');

    await complete("/Users/Music/Practice tracks");
    await app.click('[aria-label="Close practice track completion"]');
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    await app.click('[data-action="practice-export"]');
    await app.event("practice-progress", { id: "practice", completed: 0, running: 1, total: 1, stage: "Rendering Violin" });
    assert.ok(tick);
    await app.remount();
    assert.equal(tick, undefined, "unmount clears the progress timer");
    await act(async () => finish(result("/output/disposed")));
    await app.settle();
    assert.equal(app.document.querySelector('[role="dialog"]'), null, "a disposed session cannot show completion");
    assert.doesNotMatch(app.document.body.textContent!, /disposed/);
  } finally { await app.close(); }
});

test("compact Practice exposes mix and renderer settings with focus restoration", async () => {
  const app = await mountTestApp(async (command) => {
    if (command === "open_files") return ["/scores/practice.mscz"];
    if (command === "release_document") return;
    if (command === "inspect_file") return {
      id: "practice", name: "practice.mscz", path: "/scores/practice.mscz", format: "musescore", parts: [], error: null,
      practice_parts: [{ key: "one", name: "Violin", visible: true, original_instruments: [], existing_preset: { bank: 0, program: 40 } }],
    };
    if (command === "practice_environment") return {
      executable: "/bin/mscore", soundfont: "/sounds/MS Basic.sf3", version: "4.7.5", error: null,
      presets: [{ bank: 0, program: 40, name: "Violin" }],
    };
    throw new Error(`Unexpected command ${command}`);
  });
  const originalMatchMedia = app.browser.matchMedia.bind(app.browser);
  app.browser.matchMedia = ((query: string) => {
    const media = originalMatchMedia(query);
    if (query === "(max-width: 900px)") Object.defineProperty(media, "matches", { value: true });
    return media;
  }) as typeof app.browser.matchMedia;
  try {
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    assert.equal(app.document.querySelector("#background-reduction"), null, "mix controls live in the compact dialog");
    await app.click(".practice-compact-settings");
    assert.match(app.find('[role="dialog"]').textContent!, /Mix & renderer/);
    assert.equal(app.find<HTMLInputElement>("#background-reduction").value, "6");
    await app.input("#background-reduction", "11");
    await app.click('[data-action="practice-settings"]');
    assert.equal(app.document.querySelectorAll('[role="dialog"]').length, 1);
    assert.match(app.find('[role="dialog"]').textContent!, /MuseScore settings/);
    await app.click('[aria-label="Close MuseScore settings"]');
    assert.equal(app.document.activeElement, app.find(".practice-compact-settings"));
    await app.click(".practice-compact-settings");
    assert.equal(app.find<HTMLInputElement>("#background-reduction").value, "11");
    await act(async () => { app.document.activeElement?.dispatchEvent(new app.browser.KeyboardEvent("keydown", { key: "Escape", bubbles: true })); });
    await app.settle();
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.equal(app.document.activeElement, app.find(".practice-compact-settings"));
  } finally { await app.close(); }
});

test("MuseScore settings gate instruments during detection, path changes and renderer failures", async () => {
  let complete: (environment: PracticeEnvironment) => void;
  let fail: (error: Error) => void;
  const ready: PracticeEnvironment = {
    executable: "/bin/mscore", soundfont: "/sounds/MS Basic.sf3", version: "4.7.5", error: null,
    presets: [{ bank: 0, program: 40, name: "Violin" }],
  };
  const app = await mountTestApp(async (command) => {
    if (command === "open_files") return ["/scores/practice.mscz"];
    if (command === "release_document") return;
    if (command === "inspect_file") return {
      id: "practice", name: "practice.mscz", path: "/scores/practice.mscz", format: "musescore", parts: [], error: null,
      practice_parts: [{ key: "one", name: "Violin", visible: true, original_instruments: ["Violin"], existing_preset: { bank: 0, program: 40 } }],
    };
    if (command === "practice_environment") return new Promise<PracticeEnvironment>((resolve, reject) => { complete = resolve; fail = reject; });
    throw new Error(`Unexpected command ${command}`);
  });
  const assertDisabled = (disabled: boolean) => {
    assert.equal(app.find('[aria-label="Practice instruments"]').getAttribute("aria-disabled"), String(disabled));
    for (const input of app.document.querySelectorAll('[aria-label="Practice instruments"] input'))
      assert.equal(input.hasAttribute("disabled"), disabled);
    assert.equal(app.find<HTMLButtonElement>('[data-action="practice-export"]').disabled, disabled);
  };
  try {
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    assert.match(app.find(".practice-environment").textContent!, /Checking MuseScore/);
    assertDisabled(true);
    await app.click('[data-action="practice-settings"]');
    assert.ok(app.find<HTMLButtonElement>('[data-action="practice-detect"]').disabled);
    await act(async () => complete(ready));
    await app.settle();
    assertDisabled(false);
    await app.input(".practice-path input", "/custom/mscore");
    assertDisabled(true);
    assert.doesNotMatch(app.find(".practice-environment").textContent!, /Found and using/);
    await app.click('[data-action="practice-detect"]');
    assertDisabled(true);
    await act(async () => fail(new Error("Could not launch MuseScore")));
    await app.settle();
    assertDisabled(true);
    assert.match(app.find('[role="dialog"] [role="alert"]').textContent!, /Could not launch MuseScore/);
    await app.click('[data-action="practice-detect"]');
    await act(async () => complete({ ...ready, soundfont: null, error: "Matching MS Basic SoundFont was not found" }));
    await app.settle();
    assertDisabled(true);
    assert.match(app.find('[role="dialog"] [role="alert"]').textContent!, /SoundFont was not found/);
    await app.click('[data-action="practice-detect"]');
    await act(async () => complete(ready));
    await app.settle();
    await app.click('[aria-label="Close MuseScore settings"]');
    assert.match(app.find(".practice-environment").textContent!, /Found and using MuseScore Version 4.7.5/);
    assertDisabled(false);
  } finally { await app.close(); }
});

test("explicit practice defaults update each mode independently and survive score loads and app restarts", async () => {
  const app = await mountTestApp(async (command) => {
    if (command === "open_files") return ["/scores/practice.mscz"];
    if (command === "release_document") return;
    if (command === "inspect_file") return {
      id: "practice", name: "practice.mscz", path: "/scores/practice.mscz", format: "musescore", parts: [], error: null,
      practice_parts: [true, true, false].map((visible, index) => ({
        key: String(index), name: `Part ${index}`, visible, original_instruments: [], existing_preset: { bank: 0, program: 40 },
      })),
    };
    if (command === "practice_environment") return {
      executable: "/bin/mscore", soundfont: "/sounds/MS Basic.sf3", version: "4.7.5", error: null,
      presets: [{ bank: 0, program: 40, name: "Violin" }, { bank: 0, program: 0, name: "Piano" }],
    };
    throw new Error(`Unexpected command ${command}`);
  });
  const value = (id: string) => app.find<HTMLInputElement>(`#${id}`).value;
  const saved = () => JSON.parse(app.browser.localStorage.getItem(PRACTICE_DEFAULTS_KEY) ?? "{}");
  try {
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    assert.equal(app.document.querySelector('[aria-label="Set default background reduction"]'), null);
    await app.choose("#foreground_sound-0", "Piano");
    assert.equal(value("foreground_sound-1"), "Violin", "choosing a sound alone does not update other pickers");
    assert.equal(saved().foreground_sound, undefined, "choosing alone does not save a default");
    await app.click('.practice-part [aria-label="Set default foreground instrument"]');
    assert.equal(value("foreground_sound-1"), "Piano");
    assert.equal(value("background_sound-0"), "Violin", "foreground defaults leave background alone");
    assert.equal(value("background_sound-1"), "Violin");
    assert.equal(app.document.querySelector('[aria-label="Set default foreground instrument"]'), null);
    assert.deepEqual(saved().foreground_sound, { bank: 0, program: 0 });

    await app.choose("#background_sound-0", "Piano");
    await app.click('[aria-label="Set default background instrument"]');
    assert.equal(value("background_sound-1"), "Piano");
    await app.choose("#foreground_sound-0", "Violin");
    await app.click('[aria-label="Set default foreground instrument"]');
    assert.equal(value("foreground_sound-1"), "Violin");
    assert.equal(value("background_sound-0"), "Piano", "background defaults are independent");
    for (const field of ["create_track", "include_in_background"]) await app.click(`[data-practice-index="2"][data-practice-field="${field}"]`);
    assert.equal(value("foreground_sound-2"), "Violin", "defaults also update inactive pickers");
    assert.equal(value("background_sound-2"), "Piano");

    await app.click("#practice-metronome");
    await app.input("#background-reduction", "9");
    await app.input("#metronome-level", "-12");
    assert.equal(saved().metronome_enabled, true, "the checkbox saves automatically");
    assert.equal(saved().background_reduction_db, undefined);
    assert.equal(saved().metronome_db, undefined);
    await app.click('[aria-label="Set default background reduction"]');
    assert.equal(saved().background_reduction_db, 9);
    assert.equal(saved().metronome_db, undefined, "saving reduction does not save metronome level");
    await app.click('[aria-label="Set default metronome level"]');
    assert.equal(saved().metronome_db, -12);
    assert.equal(app.document.querySelector('[aria-label="Set default background reduction"]'), null);
    assert.equal(app.document.querySelector('[aria-label="Set default metronome level"]'), null);
    await app.input("#background-reduction", "15");
    await app.input("#metronome-level", "-20");
    await app.choose("#foreground_sound-0", "Piano");

    const assertDefaults = () => {
      assert.equal(value("foreground_sound-0"), "Violin");
      assert.equal(value("background_sound-0"), "Piano");
      assert.equal(value("background-reduction"), "9");
      assert.equal(value("metronome-level"), "-12");
      assert.equal(app.find<HTMLInputElement>("#practice-metronome").checked, true);
      assert.ok(app.find<HTMLInputElement>("#foreground_sound-2").disabled, "membership choices remain per score");
    };
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    assertDefaults();
    await app.remount();
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    assertDefaults();
    await app.click("#practice-metronome");
    await app.remount();
    await app.menu("open-score");
    await app.click('[data-mode="practice"]');
    assert.equal(app.find<HTMLInputElement>("#practice-metronome").checked, false, "a disabled metronome is remembered too");
    assert.equal(value("metronome-level"), "-12");
  } finally { await app.close(); }
});
