import assert from "node:assert/strict";
import test from "node:test";
import { act } from "react";
import type { Inspection, Operation } from "./model.ts";
import { mountTestApp } from "./test-support.ts";

function score(path: string, error: string | null = null): Inspection {
  return {
    id: path, path, name: path.split("/").pop()!, error, format: "musicxml",
    parts: ["P1", "P2", "P3", "P4"].map((id) => ({
      id, name: "Violin", instrument_names: ["Violin"], voice_count: 3,
      split_plans: {
        voices: [
          { voice: "1", tone: 0, fallback_voice: null },
          { voice: "2", tone: 0, fallback_voice: null },
          { voice: "3", tone: 0, fallback_voice: "2" },
        ],
        chords: [
          { voice: "1", tone: 0, fallback_voice: null },
          { voice: "2", tone: 0, fallback_voice: null },
          { voice: "2", tone: 1, fallback_voice: null },
        ],
      },
    })),
  };
}

test("browser preview still renders when native event APIs are unavailable", async () => {
  const app = await mountTestApp(() => { throw new Error("No native commands in a browser"); });
  const browser = app.browser as unknown as { __TAURI_INTERNALS__?: unknown };
  const internals = browser.__TAURI_INTERNALS__;
  const container = app.document.createElement("div");
  app.document.body.append(container);
  let dispose: (() => void) | undefined;
  try {
    delete browser.__TAURI_INTERNALS__;
    const { mountApp } = await app.server.ssrLoadModule("/src/app.tsx");
    await act(async () => { dispose = mountApp(container as unknown as HTMLElement); });
    assert.ok(container.querySelector(".empty"));
    assert.match(container.querySelector(".status")!.textContent!, /Native.*unavailable/);
  } finally {
    await act(async () => { dispose?.(); });
    browser.__TAURI_INTERNALS__ = internals;
    await app.close();
  }
});

test("single-score lifecycle preserves a score on cancellation or failure and releases native sessions", async () => {
  let nextPath: string | null = "/scores/first.xml";
  let finishInspection: ((inspection: Inspection) => void) | undefined;
  let heldInspection = false;
  let failReleaseId: string | undefined;
  const released: string[] = [];
  const inspected: string[] = [];
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_files") return nextPath ? [nextPath] : null;
    if (command === "release_document") {
      if (args.id === failReleaseId) { failReleaseId = undefined; throw new Error("Native session is busy"); }
      released.push(String(args.id)); return;
    }
    if (command === "inspect_file") {
      const path = String(args.path); inspected.push(path);
      if (heldInspection) return await new Promise<Inspection>((resolve) => { finishInspection = resolve; });
      if (path.includes("throws")) throw new Error("Unreadable replacement");
      return score(path, path.includes("broken") ? "Part P2: invalid duration" : null);
    }
    throw new Error(`Unexpected command ${command}`);
  });
  try {
    assert.equal(app.document.querySelector('[data-action="close"]'), null);
    assert.equal(app.document.querySelector('header button'), null, "file and update actions live in the native menu");
    assert.equal(app.fileMenu.openEnabled, true);
    assert.equal(app.fileMenu.closeEnabled, false);
    assert.equal(app.find(".status").textContent, "", "landing page has no device slogan");
    assert.equal(app.find<HTMLButtonElement>('[data-action="open"]').classList.contains("button"), true);
    const css = await app.server.transformRequest("/src/style.css");
    assert.ok(css?.code.includes(".button--primary"), "HeroUI styles are compiled");
    await app.menu("open-score");
    assert.match(app.document.body.textContent!, /first.xml/);
    assert.equal(app.fileMenu.closeEnabled, true);
    assert.equal(app.find(".status").textContent, "", "loaded screens have no device slogan");
    assert.equal(app.document.querySelector("#split-mode"), null, "loading stops at mode selection");
    assert.equal(app.document.querySelectorAll("[data-mode]").length, 4);
    assert.equal(app.document.querySelector('input[type="radio"]'), null, "mode tiles replace the radio section");
    assert.ok(app.find<HTMLButtonElement>('[data-mode="practice"]').disabled, "MusicXML cannot create practice tracks");
    nextPath = null;
    await app.menu("open-score");
    assert.equal(app.document.querySelectorAll("[data-mode]").length, 4, "cancelling replacement keeps mode selection");
    await app.click('[data-mode="split"]');
    assert.equal(app.document.activeElement, app.find("#mode-config-heading"));
    for (const checkbox of app.document.querySelectorAll(".card .checkbox"))
      assert.ok(checkbox.classList.contains("checkbox--secondary"), "checkboxes on cards use the surface variant");
    assert.ok(app.find(".select").classList.contains("select--secondary"), "selects on cards use the surface variant");
    await app.click('[data-part="P2"]');
    assert.equal(app.document.activeElement, app.find('[data-part="P2"] input'), "routine changes do not steal focus");
    await app.click("#insert-fallback-slurs");
    await app.click('[data-action="back-to-modes"]');
    assert.equal(app.document.activeElement, app.find('[data-mode="split"]'));
    assert.equal(app.document.querySelector("#split-mode"), null);
    await app.click('[data-mode="split"]');
    assert.ok(app.find(".split-preview"), "reopening the same mode keeps selected parts");
    assert.equal(app.find<HTMLInputElement>("#insert-fallback-slurs").checked, false);
    nextPath = null;
    await app.menu("open-score");
    assert.equal(app.find<HTMLInputElement>("#insert-fallback-slurs").checked, false);
    assert.equal(released.length, 0);
    nextPath = "/scores/broken.xml";
    await app.menu("open-score");
    assert.match(app.find('[role="alert"]').textContent!, /invalid duration/);
    assert.match(app.document.body.textContent!, /first.xml/);
    assert.equal(app.find<HTMLInputElement>("#insert-fallback-slurs").checked, false);
    assert.ok(released.includes("/scores/broken.xml"), "failed replacement session released");
    nextPath = "/scores/throws.xml";
    await app.menu("open-score");
    assert.match(app.find('[role="alert"]').textContent!, /Unreadable replacement/);
    assert.match(app.document.body.textContent!, /first.xml/);
    failReleaseId = "/scores/first.xml";
    nextPath = "/scores/unused.xml";
    await app.menu("open-score");
    assert.match(app.find('[role="alert"]').textContent!, /Native session is busy/);
    assert.match(app.document.body.textContent!, /first.xml/);
    assert.ok(released.includes("/scores/unused.xml"), "candidate released if previous session cannot be released");
    failReleaseId = "/scores/first.xml";
    await app.menu("close-score");
    assert.match(app.find('[role="alert"]').textContent!, /Native session is busy/);
    assert.match(app.document.body.textContent!, /first.xml/);
    await app.event("tauri://drag-drop", { paths: ["/scores/a.xml", "/scores/b.xml", "/scores/c.xml"], position: { x: 0, y: 0 } });
    assert.match(app.find('[role="alert"]').textContent!, /one score.*two scores/i);
    assert.equal(inspected.includes("/scores/a.xml"), false);
    heldInspection = true;
    nextPath = "/scores/second.xml";
    await app.menu("open-score");
    assert.ok(!app.fileMenu.openEnabled);
    assert.ok(!app.fileMenu.closeEnabled);
    await app.event("menu-action", "close-score");
    await app.event("menu-action", "open-score");
    assert.equal(released.includes("/scores/first.xml"), false, "queued menu events cannot close a busy score");
    assert.ok(app.find<HTMLButtonElement>('[data-action="back-to-modes"]').disabled);
    await app.event("tauri://drag-drop", { paths: ["/scores/ignored.xml"], position: { x: 0, y: 0 } });
    assert.equal(inspected.includes("/scores/ignored.xml"), false);
    await act(async () => { finishInspection!(score(nextPath!)); });
    await app.settle();
    heldInspection = false;
    assert.ok(released.includes("/scores/first.xml"));
    assert.match(app.document.body.textContent!, /second.xml/);
    assert.equal(app.document.querySelector("#insert-fallback-slurs"), null, "replacement returns to mode selection");
    await app.click('[data-mode="split"]');
    assert.ok(app.find<HTMLInputElement>("#insert-fallback-slurs").checked, "new score resets choices");
    assert.equal(app.document.querySelector(".split-preview"), null, "new score starts without selected parts");
    await app.menu("close-score");
    assert.ok(released.includes("/scores/second.xml"));
    assert.equal(app.find(".status").textContent, "", "returning to the landing page clears the slogan");
    assert.equal(app.document.querySelector('[data-part="P1"]'), null);
    await app.event("tauri://drag-drop", { paths: ["/scores/dropped.xml"], position: { x: 0, y: 0 } });
    assert.match(app.document.body.textContent!, /dropped.xml/);
    assert.ok(app.find('[data-mode="split"]'), "a dropped score also enters mode selection");
    assert.equal(app.registrations.filter((event) => event === "tauri://drag-drop").length, 1);
    assert.equal(app.registrations.filter((event) => event === "practice-progress").length, 1);
    assert.equal(app.registrations.filter((event) => event === "menu-action").length, 1);
  } finally { await app.close(); }
  assert.equal(app.unregistered.length, app.registrations.length, "listeners cleaned up on unmount");
});

test("HeroUI split and merge controls retain choices, validate, and send unchanged export payloads", async () => {
  const exported: { id: string; operation: Operation }[] = [];
  let result: { path: string; warnings: string[] } | null = null;
  let exportError: string | undefined;
  let finishExport: (() => void) | undefined;
  let holdExport = false;
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_files") return ["/scores/orchestra.xml"];
    if (command === "inspect_file") return score(String(args.path));
    if (command === "release_document") return;
    if (command === "export_document") {
      exported.push(args as unknown as (typeof exported)[number]);
      if (holdExport) await new Promise<void>((resolve) => { finishExport = resolve; });
      if (exportError) throw exportError;
      return result;
    }
    throw new Error(`Unexpected command ${command}`);
  });
  try {
    await app.menu("open-score");
    await app.click('[data-mode="split"]');
    await app.click('[data-action="export"]');
    assert.match(app.find('[role="alert"]').textContent!, /Select at least one/);
    assert.equal(exported.length, 0);
    await app.click('[data-part="P2"]');
    assert.match(app.find(".split-preview").textContent!, /3 additional staves/);
    assert.equal(app.find('[data-part="P2"] .part-name').textContent, "Violin", "instrument names have no repeated subtitle or internal ID");
    assert.deepEqual(Array.from(app.find(".split-preview").querySelectorAll(".score-select [data-slot='label']"), (label) => label.textContent), ["Fallback voice", "Fallback voice", "Fallback voice"]);
    assert.equal(app.document.querySelectorAll(".fallback-row[role='group'][aria-labelledby]").length, 3, "staff context stays separate from the selector label");
    assert.match(app.find(".options-card").textContent!, /Matching pitches continue existing ties/);
    await app.choose("#fallback-P2-2-1", "Voice 3");
    await app.click('[data-action="export"]');
    assert.deepEqual(exported.at(-1)?.operation, {
      kind: "split", part_ids: ["P2"], mode: "chords", insert_fallback_slurs: true,
      fallbacks: [
        { part_id: "P2", voice: "1", tone: 0, fallback_voice: null },
        { part_id: "P2", voice: "2", tone: 0, fallback_voice: null },
        { part_id: "P2", voice: "2", tone: 1, fallback_voice: "3" },
      ],
    });
    await app.click("#insert-fallback-slurs");
    await app.choose("#split-mode", "Explicit voices");
    await app.choose("#fallback-P2-3-0", "None (silent)");
    await app.click('[data-part="P2"]');
    await app.click('[data-part="P2"]');
    holdExport = true;
    await app.click('[data-action="export"]');
    assert.ok(!app.fileMenu.openEnabled);
    assert.ok(!app.fileMenu.closeEnabled);
    assert.ok(app.find<HTMLInputElement>("#insert-fallback-slurs").disabled);
    assert.ok(app.find<HTMLButtonElement>('[data-action="back-to-modes"]').disabled);
    await act(async () => { finishExport!(); });
    holdExport = false; await app.settle();
    assert.deepEqual(exported.at(-1)?.operation, {
      kind: "split", part_ids: ["P2"], mode: "voices", insert_fallback_slurs: false,
      fallbacks: ["1", "2", "3"].map((voice) => ({ part_id: "P2", voice, tone: 0, fallback_voice: null })),
    });
    assert.match(app.find(".status").textContent!, /cancelled/);
    result = { path: "/scores/arranged.mxl", warnings: ["Ambiguous tie assignment; affected passage marked red"] };
    await app.click('[data-action="export"]');
    assert.match(app.find(".status").textContent!, /Exported.*arranged.mxl/s);
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.match(app.find(".status").textContent!, /Ambiguous tie assignment/);
    exportError = "Cannot write output file";
    await app.click('[data-action="export"]');
    assert.match(app.find('[role="alert"]').textContent!, /Cannot write output file/);
    assert.equal(app.find<HTMLButtonElement>('[data-action="export"]').disabled, false);
    exportError = undefined;
    await app.click('[data-action="back-to-modes"]');
    await app.click('[data-mode="merge"]');
    assert.match(app.find(".merge-empty").textContent!, /Select two or more instruments/);
    await app.click('[data-part="P1"]');
    await app.click('[data-part="P2"]');
    await app.click('[data-action="add-group"]');
    await app.click('[data-action="down"][data-group="0"][data-index="0"]');
    await app.click('[data-part="P3"]');
    await app.click('[data-part="P4"]');
    await app.click('[data-action="add-group"]');
    assert.equal(app.document.querySelectorAll(".merge-group").length, 2);
    assert.ok(app.find(".merge-part").querySelector("#stem-P2"), "reordered instruments retain the correct stem controls");
    assert.deepEqual(Array.from(app.document.querySelectorAll(".merge-group .score-select [data-slot='label']"), (label) => label.textContent), ["Stem direction", "Stem direction", "Stem direction", "Stem direction"]);
    assert.equal(app.find(".group-part [data-slot='description']").textContent, "Voice 1", "voice context has no internal part ID");
    await app.choose("#stem-P1", "Up");
    await app.choose("#stem-P3", "Down");
    await app.click('[data-action="up"][data-group="1"][data-index="1"]');
    await app.click('[data-action="export"]');
    assert.deepEqual(exported.at(-1)?.operation, {
      kind: "merge", groups: [["P2", "P1"], ["P4", "P3"]], stems: [["up", "up"], ["up", "down"]],
    });
    await app.click('[data-action="remove-group"][data-group="1"]');
    assert.equal(app.document.querySelectorAll(".merge-group").length, 1);
  } finally { await app.close(); }
});
