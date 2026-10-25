import assert from "node:assert/strict";
import test from "node:test";
import { act } from "react";
import type { Inspection } from "./model.ts";
import { RECENT_SCORES_KEY } from "./recent-scores.ts";
import { mountTestApp } from "./test-support.ts";

const trigger = 'button[aria-label="Recently opened scores"]';
const choose = '.empty [data-action="open"]';
const score = (path: string): Inspection => ({
  id: path, path, name: path.split(/[\\/]/).pop()!, format: "musicxml", parts: [], error: null,
});

test("landing split button checks recent files, reopens them, and handles keyboard, loading, errors, and busy states", async () => {
  const checked: string[][] = [];
  const inspected: string[] = [];
  const paths = ["/scores/first.xml", "/deleted.xml", "C:\\scores\\first.xml"];
  let picked: string | null = "/scores/picked.xml";
  let pickerCalls = 0;
  let failCheck = false;
  let holdCheck = false;
  let finishCheck: ((paths: string[]) => void) | undefined;
  let holdInspection = false;
  let finishInspection: ((value: Inspection) => void) | undefined;
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_files") { pickerCalls++; return picked ? [picked] : null; }
    if (command === "release_document") return;
    if (command === "existing_score_paths") {
      checked.push(args.paths as string[]);
      if (failCheck) throw new Error("Unavailable");
      if (holdCheck) return new Promise<string[]>((resolve) => { finishCheck = resolve; });
      return (args.paths as string[]).filter((path) => path !== "/deleted.xml");
    }
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
  const items = () => Array.from(app.document.querySelectorAll('[role="menuitem"]'));
  const key = async (value: string) => {
    await act(async () => {
      app.document.activeElement!.dispatchEvent(new app.browser.KeyboardEvent("keydown", { key: value, bubbles: true }));
      app.document.activeElement!.dispatchEvent(new app.browser.KeyboardEvent("keyup", { key: value, bubbles: true }));
    });
    await app.settle();
  };
  try {
    assert.equal(app.find('[aria-label="Open a score"]').querySelectorAll("button").length, 2);
    assert.equal(app.find(choose).getAttribute("aria-haspopup"), null, "left button only opens the file picker");
    await app.click(trigger);
    assert.match(app.find('[role="menu"]').textContent!, /No recent scores/);
    assert.equal(pickerCalls, 0);
    await key("Escape");
    assert.equal(app.find(trigger).getAttribute("aria-expanded"), "false");
    assert.equal(app.document.activeElement, app.find(trigger));
    await app.click(trigger);
    await act(async () => {
      const outside = app.find("h1");
      outside.dispatchEvent(new app.browser.PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }) as unknown as Event);
      outside.dispatchEvent(new app.browser.PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }) as unknown as Event);
      outside.click();
    });
    await app.settle();
    assert.equal(app.find(trigger).getAttribute("aria-expanded"), "false", "outside click dismisses the menu");

    app.browser.localStorage.setItem(RECENT_SCORES_KEY, JSON.stringify(paths));
    await app.remount();
    holdCheck = true;
    await app.click(trigger);
    assert.match(app.find('[role="menu"]').textContent!, /Checking recent scores/);
    assert.ok(!app.find('[role="menu"]').textContent!.includes("first.xml"), "unchecked history stays hidden");
    assert.deepEqual(checked.at(-1), paths);
    await act(async () => { finishCheck!([paths[0], paths[2]]); });
    await app.settle();
    holdCheck = false;
    assert.equal(items().length, 2);
    assert.ok(items()[0].textContent!.includes(paths[0]));
    assert.ok(items()[1].textContent!.includes(paths[2]));
    assert.ok(!app.find('[role="menu"]').textContent!.includes("/deleted.xml"));
    await key("ArrowDown");
    await key("End");
    await key("Enter");
    assert.deepEqual(inspected, [paths[2]], "keyboard selection reopens the selected path");
    assert.equal(pickerCalls, 0, "recent selection bypasses the picker");
    assert.equal(history()[0], paths[2]);
    await app.click('[data-action="close"]');

    await app.click(trigger);
    await app.click('[role="menuitem"]');
    assert.equal(inspected.at(-1), paths[2], "mouse selection reopens the same score");
    await app.click('[data-action="close"]');
    await app.click(choose);
    assert.equal(pickerCalls, 1);
    assert.equal(history()[0], "/scores/picked.xml");
    await app.click('[data-action="close"]');
    const saved = history();
    picked = null;
    await app.click(choose);
    assert.deepEqual(history(), saved, "picker cancellation leaves history unchanged");
    picked = "/scores/broken.xml";
    await app.click(choose);
    assert.deepEqual(history(), saved, "failed inspection leaves history unchanged");

    failCheck = true;
    await app.click(trigger);
    assert.match(app.find('[role="menu"]').textContent!, /Recent scores unavailable/);
    assert.equal(items().length, 1, "validation errors never show unchecked paths");
    await key("Escape");
    failCheck = false;
    holdCheck = true;
    await app.click(trigger);
    await key("Escape");
    await act(async () => { finishCheck!(["/stale.xml"]); });
    await app.settle();
    await app.click(trigger);
    assert.match(app.find('[role="menu"]').textContent!, /Checking recent scores/);
    assert.ok(!app.find('[role="menu"]').textContent!.includes("stale.xml"));
    await key("Escape");
    await app.remount();
    await act(async () => { finishCheck!(["/after-disposal.xml"]); });
    await app.settle();
    holdCheck = false;

    holdInspection = true;
    picked = "/scores/busy.xml";
    await app.click(choose);
    assert.ok(app.find<HTMLButtonElement>(choose).disabled);
    assert.ok(app.find<HTMLButtonElement>(trigger).disabled);
    await act(async () => { finishInspection!(score(picked!)); });
    await app.settle();
    holdInspection = false;
    await app.click('[data-action="close"]');
    await app.event("tauri://drag-drop", { paths: ["/scores/drop.xml"], position: { x: 0, y: 0 } });
    assert.equal(history()[0], "/scores/drop.xml", "drops join the shared history");
    await app.click('[data-action="close"]');
    await app.remount();
    await app.click(trigger);
    assert.ok(items()[0].textContent!.includes("/scores/drop.xml"), "history persists across remounts");
    assert.ok(!app.find('[role="menu"]').textContent!.includes("after-disposal.xml"));
  } finally { await app.close(); }
});
