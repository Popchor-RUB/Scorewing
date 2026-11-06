import assert from "node:assert/strict";
import test from "node:test";
import { act } from "react";
import type { Inspection } from "./model.ts";
import { RECENT_SCORES_KEY } from "./recent-scores.ts";
import { mountTestApp } from "./test-support.ts";

const choose = '[data-action="open"]';
const score = (path: string): Inspection => ({
  id: path, path, name: path.split(/[\\/]/).pop()!, format: "musicxml", parts: [], error: null,
});

test("start page matches the design and preserves picker, busy, drop, and history behavior", async () => {
  const inspected: string[] = [];
  let picked: string | null = "/scores/picked.xml";
  let pickerCalls = 0;
  let holdInspection = false;
  let finishInspection: ((value: Inspection) => void) | undefined;
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_files") { pickerCalls++; return picked ? [picked] : null; }
    if (command === "release_document") return;
    if (command === "existing_score_paths") return args.paths;
    if (command === "inspect_file") {
      const path = String(args.path);
      inspected.push(path);
      if (holdInspection) return new Promise<Inspection>((resolve) => { finishInspection = resolve; });
      if (path === "/scores/broken.xml") throw new Error("Cannot read score");
      return score(path);
    }
    throw new Error(`Unexpected command ${command}`);
  });
  const history = () => JSON.parse(app.browser.localStorage.getItem(RECENT_SCORES_KEY) ?? "[]") as string[];
  try {
    const start = app.find(".empty");
    assert.ok(start.firstElementChild!.classList.contains("drop-hint"));
    assert.equal(app.find("#recent-scores-heading").textContent, "Last opened scores");
    assert.equal(app.document.querySelector(".recent-scores > header p"), null);
    assert.equal(app.document.querySelector(".app-header"), null, "start page has no empty toolbar");
    assert.equal(app.document.querySelector(".shortcut"), null);
    assert.equal(app.find(".drop-actions strong").textContent, "Drop a score here, or");
    assert.equal(app.find(".drop-hint").querySelectorAll(choose).length, 1, "Open score belongs to the dropzone");
    assert.ok(app.find(`${choose} .lucide-folder-closed[data-icon="inline-start"]`));
    assert.equal(app.find(choose).getAttribute("data-variant"), "default");
    assert.match(app.find(".recent-scores").textContent!, /No recent scores/);

    await app.click(choose);
    assert.equal(pickerCalls, 1);
    assert.deepEqual(history(), [picked]);
    assert.ok(app.find(`.app-header ${choose}`), "loaded scores retain the toolbar Open command");
    await app.menu("close-score");
    const saved = history();
    picked = null;
    await app.click(choose);
    assert.deepEqual(history(), saved, "picker cancellation leaves history unchanged");
    picked = "/scores/broken.xml";
    await app.click(choose);
    assert.deepEqual(history(), saved, "failed inspection leaves history unchanged");
    assert.match(app.find('[role="alert"]').textContent!, /Cannot read score/);

    holdInspection = true;
    picked = "/scores/busy.xml";
    await app.click(choose);
    assert.ok(app.find<HTMLButtonElement>(choose).disabled);
    assert.equal(app.find(choose).textContent, "Reading your score…");
    assert.ok(app.find<HTMLButtonElement>(".recent-score-row").disabled);
    await app.event("tauri://drag-drop", { paths: ["/scores/ignored.xml"], position: { x: 0, y: 0 } });
    assert.equal(inspected.includes("/scores/ignored.xml"), false);
    await act(async () => { finishInspection!(score(picked!)); });
    await app.settle();
    holdInspection = false;
    await app.menu("close-score");

    await app.event("tauri://drag-drop", { paths: ["/scores/drop.xml"], position: { x: 0, y: 0 } });
    assert.equal(history()[0], "/scores/drop.xml", "drops join the shared history");
    await app.menu("close-score");
    await app.remount();
    assert.match(app.find(".recent-score-row").textContent!, /drop.xml/);
    const callsBeforeReopen = pickerCalls;
    await app.click(".recent-score-row");
    assert.equal(inspected.at(-1), "/scores/drop.xml");
    assert.equal(pickerCalls, callsBeforeReopen, "history selection bypasses the picker");
    assert.ok(app.find('[data-mode="split"]'));
  } finally { await app.close(); }
});

test("start page validates recent rows, retries failures, and opens an existing score", async () => {
  const paths = ["/scores/first.xml", "/deleted.xml", "C:\\scores\\second.mxl"];
  let fail = true;
  const opened: string[] = [];
  const app = await mountTestApp((command, args) => {
    if (command === "existing_score_paths") {
      if (fail) throw new Error("Unavailable");
      return (args.paths as string[]).filter((path) => path !== "/deleted.xml");
    }
    if (command === "inspect_file") { opened.push(String(args.path)); return score(String(args.path)); }
    if (command === "release_document") return;
    throw new Error(`Unexpected command ${command}`);
  }, { initialize: (browser) => browser.localStorage.setItem(RECENT_SCORES_KEY, JSON.stringify(paths)) });
  try {
    assert.match(app.find('[aria-labelledby="recent-scores-heading"]').textContent!, /Recent scores unavailable/);
    assert.equal(app.document.querySelectorAll(".recent-score-row").length, 0);
    fail = false;
    await app.click('[aria-labelledby="recent-scores-heading"] button');
    const rows = Array.from(app.document.querySelectorAll(".recent-score-row"));
    assert.equal(rows.length, 2);
    assert.match(rows[0].textContent!, /first.xml.*\/scores/);
    assert.match(rows[1].textContent!, /second.mxl.*C:\\scores/);
    assert.ok(!app.find('[aria-labelledby="recent-scores-heading"]').textContent!.includes("deleted.xml"));
    await app.click(".recent-score-row");
    assert.deepEqual(opened, [paths[0]]);
    assert.ok(app.find('[data-mode="split"]'));
  } finally { await app.close(); }
});
