import assert from "node:assert/strict";
import test from "node:test";
import { act } from "react";
import { newArrangement, operation, type Inspection } from "./model.ts";
import { mountTestApp } from "./test-support.ts";

function score(path: string): Inspection {
  return { id: path, path, name: path.split("/").pop()!, error: null, format: "musicxml", parts: [] };
}

test("diff requires an older version and produces the desktop operation", () => {
  const arrangement = newArrangement();
  arrangement.kind = "diff";
  assert.throws(() => operation(arrangement, []), /older MusicXML/);
  assert.deepEqual(operation(arrangement, [], "older-id"), { kind: "diff", older_id: "older-id" });
});

test("inline diff preserves selections through cancellation and failure, replaces either file, and clears after export", async () => {
  let path: string | null = "/scores/newer.musicxml";
  const released: string[] = [];
  const requests: Record<string, unknown>[] = [];
  let result: { path: string; warnings: string[] } | null = null;
  let failExport = false;
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return path ? [path] : null;
    if (command === "open_file") return path;
    if (command === "prepare_musicxml") throw new Error("Conversion failed");
    if (command === "inspect_file") {
      const candidate = score(String(args.path));
      if (candidate.path.includes("broken")) candidate.error = "Malformed XML";
      if (candidate.path.includes("mscz")) candidate.format = "musescore";
      return candidate;
    }
    if (command === "release_document") { released.push(String(args.id)); return; }
    if (command === "export_document") { requests.push(args); if (failExport) throw new Error("Export failed"); return result; }
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.menu("open-score");
    await app.click('[data-mode="diff"]');
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.ok(app.find<HTMLButtonElement>('[data-action="export"]').disabled);
    assert.ok(app.find<HTMLButtonElement>('[data-action="swap-diff"]').disabled);
    assert.match(app.find(".diff-card").textContent!, /Newer version.*newer.musicxml/);
    path = "/scores/older.mxl";
    await app.click('[data-action="open-older"]');
    assert.ok(!app.find<HTMLButtonElement>('[data-action="export"]').disabled);
    for (const replacement of [null, "/scores/broken.xml", "/scores/wrong.mscz"]) {
      path = replacement;
      await app.click('[data-action="open-older"]');
      assert.match(app.find(".diff-card").textContent!, /older.mxl/);
      assert.ok(!released.includes("/scores/older.mxl"));
    }
    assert.ok(released.includes("/scores/broken.xml"));
    assert.ok(released.includes("/scores/wrong.mscz"));
    await app.click('[data-action="export"]');
    assert.deepEqual(requests.at(-1), { id: "/scores/newer.musicxml", operation: { kind: "diff", older_id: "/scores/older.mxl" } });
    assert.match(app.find(".status").textContent!, /Export cancelled/);
    failExport = true;
    await app.click('[data-action="export"]');
    assert.match(app.find('.status[role="alert"]').textContent!, /Export failed/);
    assert.match(app.find('[data-version="old"]').textContent!, /older.mxl/);
    assert.match(app.find('[data-version="new"]').textContent!, /newer.musicxml/);
    failExport = false;
    path = "/scores/earlier.xml";
    await app.click('[data-action="open-older"]');
    assert.ok(released.includes("/scores/older.mxl"));
    await app.click('[data-action="back-to-modes"]');
    await app.click('[data-mode="diff"]');
    assert.match(app.find(".diff-card").textContent!, /earlier.xml/, "reopening diff retains the comparison document");
    await app.click('[data-action="back-to-modes"]');
    await app.click('[data-mode="merge"]');
    await app.click('[data-action="back-to-modes"]');
    await app.click('[data-mode="diff"]');
    assert.match(app.find(".diff-card").textContent!, /earlier.xml/);
    path = "/scores/replacement.xml";
    await app.click('[data-action="open-newer"]');
    assert.match(app.find('[data-version="new"]').textContent!, /replacement.xml/);
    assert.match(app.find('[data-version="old"]').textContent!, /earlier.xml/);
    assert.ok(released.includes("/scores/newer.musicxml"));
    await app.click('[data-action="swap-diff"]');
    result = { path: "/scores/newer-diff.mxl", warnings: ["Deleted notes: nearest surviving note marked red."] };
    await app.click('[data-action="export"]');
    assert.deepEqual(requests.at(-1), { id: "/scores/earlier.xml", operation: { kind: "diff", older_id: "/scores/replacement.xml" } });
    assert.ok(app.find(".empty"));
    assert.equal(app.document.querySelector(".diff-card"), null);
    assert.match(app.find(".status").textContent!, /newer-diff.mxl.*\nReview warnings:.*\nDeleted notes/);
    assert.ok(released.includes("/scores/earlier.xml"));
    assert.ok(released.includes("/scores/replacement.xml"));
    path = "/scores/another.xml";
    await app.menu("open-score");
    await app.click('[data-mode="diff"]');
    assert.ok(app.find<HTMLButtonElement>('[data-action="export"]').disabled);
    path = "/scores/another-old.xml";
    await app.click('[data-action="open-older"]');
    await app.menu("close-score");
    assert.ok(released.includes("/scores/another-old.xml"));
    assert.ok(released.includes("/scores/another.xml"));
  } finally { await app.close(); }
});

test("closing the app releases a pending older inspection and both active diff documents", async () => {
  let path = "/new.xml";
  let pending = false;
  let finish: ((value: Inspection) => void) | undefined;
  const released: string[] = [];
  const app = await mountTestApp(async (command, args) => {
    if (command === "open_files") return [path];
    if (command === "open_file") return path;
    if (command === "inspect_file") {
      if (pending) return new Promise<Inspection>((resolve) => { finish = resolve; });
      return score(String(args.path));
    }
    if (command === "release_document") { released.push(String(args.id)); return; }
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.menu("open-score");
    await app.click('[data-mode="diff"]');
    path = "/old.xml";
    await app.click('[data-action="open-older"]');
    pending = true;
    path = "/pending.xml";
    await app.click('[data-action="open-older"]');
    assert.ok(finish);
    await app.close();
    assert.deepEqual(released, []);
    await act(async () => { finish!(score("/pending.xml")); });
    assert.deepEqual(new Set(released), new Set(["/pending.xml", "/old.xml", "/new.xml"]));
  } catch (error) { await app.close(); throw error; }
});

test("pair picker confirms inferred order, swaps, retries conversion, and exports mixed scores", async () => {
  let paths = ["/current.xml"];
  const released: string[] = [];
  const prepared: string[] = [];
  const exports: Record<string, unknown>[] = [];
  let fail = true;
  let failExport = false;
  let result: { path: string; warnings: string[] } | null = null;
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return paths;
    if (command === "inspect_file") {
      const result = score(String(args.path));
      if (result.path.endsWith(".mscz")) { result.format = "musescore"; result.practice_parts = []; }
      return result;
    }
    if (command === "prepare_musicxml") {
      prepared.push(String(args.id));
      if (fail) throw new Error("Install MuseScore CLI");
      return { ...score(String(args.id)), format: "musescore", practice_parts: [] };
    }
    if (command === "release_document") { released.push(String(args.id)); return; }
    if (command === "export_document") { exports.push(args); if (failExport) throw new Error("Cannot save diff"); return result; }
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.menu("open-score");
    paths = ["/new.mscz", "/old.xml"];
    await app.menu("open-score");
    assert.equal(app.fileMenu.openEnabled, false);
    assert.equal(app.fileMenu.closeEnabled, false);
    await app.event("menu-action", "close-score");
    assert.deepEqual(released, [], "queued menu actions cannot close a score beneath the comparison dialog");
    assert.match(app.find('[data-version="old"]').textContent!, /old.xml/);
    assert.match(app.find('[data-version="new"]').textContent!, /new.mscz/);
    assert.match(app.find(".score-heading").textContent!, /current.xml/);
    assert.equal(app.find('[data-action="compare-diff"]').textContent?.trim(), "Export diff");
    await app.click('[data-action="swap-diff"]');
    assert.match(app.find('[data-version="old"]').textContent!, /new.mscz/);
    await app.click('[data-action="compare-diff"]');
    assert.match(app.find('[role="dialog"]').textContent!, /Install MuseScore CLI/);
    assert.equal(released.length, 0);
    fail = false;
    await app.click('[data-action="compare-diff"]');
    assert.match(app.find('[role="dialog"]').textContent!, /Export cancelled/);
    assert.match(app.find(".score-heading").textContent!, /current.xml/);
    assert.equal(app.document.querySelector(".diff-card"), null);
    assert.deepEqual(released, []);
    assert.deepEqual(exports[0], { id: "/old.xml", operation: { kind: "diff", older_id: "/new.mscz" } });
    assert.deepEqual(prepared, ["/new.mscz", "/new.mscz"]);
    failExport = true;
    await app.click('[data-action="compare-diff"]');
    assert.match(app.find('[role="dialog"]').textContent!, /Cannot save diff/);
    assert.deepEqual(released, []);
    failExport = false;
    result = { path: "/diff.xml", warnings: ["Review changed notes"] };
    await app.click('[data-action="compare-diff"]');
    assert.equal(app.document.querySelector('[data-action="compare-diff"]'), null);
    assert.equal(app.document.querySelector(".diff-card"), null);
    assert.ok(app.find(".empty"));
    assert.match(app.find(".status").textContent!, /diff.xml.*\nReview warnings:.*\nReview changed notes/);
    assert.deepEqual(new Set(released), new Set(["/current.xml", "/old.xml", "/new.mscz"]));
  } finally { await app.close(); }
  assert.deepEqual(new Set(released), new Set(["/current.xml", "/old.xml", "/new.mscz"]));
});

test("dropped pairs cancel without replacing the workspace and failed intake releases candidates", async () => {
  const released: string[] = [];
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return ["/current.xml"];
    if (command === "inspect_file") return { ...score(String(args.path)), error: String(args.path).includes("broken") ? "Bad XML" : null };
    if (command === "release_document") { released.push(String(args.id)); return; }
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.menu("open-score");
    await app.event("tauri://drag-drop", { paths: ["/one.xml", "/two.xml"], position: { x: 0, y: 0 } });
    assert.match(app.find('[data-version="old"]').textContent!, /one.xml/);
    await app.click('[data-action="cancel-diff"]');
    assert.deepEqual(new Set(released), new Set(["/one.xml", "/two.xml"]));
    assert.match(app.find(".score-heading").textContent!, /current.xml/);
    await app.event("tauri://drag-drop", { paths: ["/valid.xml", "/broken.xml"], position: { x: 0, y: 0 } });
    assert.match(app.find('[role="alert"]').textContent!, /Bad XML/);
    assert.ok(released.includes("/valid.xml"));
    assert.ok(released.includes("/broken.xml"));
    assert.ok(!released.includes("/current.xml"));
  } finally { await app.close(); }
});

test("pair picker replaces either slot, keeps selections on invalid picks, and defers MuseScore conversion until export", async () => {
  let path: string | null = null;
  const released: string[] = [];
  const prepared: string[] = [];
  const exports: Record<string, unknown>[] = [];
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return ["/old.xml", "/new.xml"];
    if (command === "open_file") return path;
    if (command === "inspect_file") return {
      ...score(String(args.path)),
      format: String(args.path).endsWith(".mscz") ? "musescore" : "musicxml",
      error: String(args.path).includes("broken") ? "Bad XML" : null,
    };
    if (command === "prepare_musicxml") { prepared.push(String(args.id)); return score(String(args.id)); }
    if (command === "export_document") { exports.push(args); return null; }
    if (command === "release_document") { released.push(String(args.id)); return; }
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.menu("open-score");
    for (const version of ["older", "newer"]) {
      for (const candidate of [null, "/broken.xml"]) {
        path = candidate;
        await app.click(`[data-action="open-${version}"]`);
        assert.match(app.find('[data-version="old"]').textContent!, /old.xml/);
        assert.match(app.find('[data-version="new"]').textContent!, /new.xml/);
        assert.ok(!released.includes("/old.xml"));
        assert.ok(!released.includes("/new.xml"));
      }
    }
    assert.equal(released.filter((id) => id === "/broken.xml").length, 2);
    path = "/replacement-old.mscz";
    await app.click('[data-action="open-older"]');
    assert.match(app.find('[data-version="old"]').textContent!, /replacement-old.mscz/);
    assert.match(app.find('[data-version="new"]').textContent!, /new.xml/);
    path = "/replacement-new.xml";
    await app.click('[data-action="open-newer"]');
    assert.match(app.find('[data-version="new"]').textContent!, /replacement-new.xml/);
    assert.deepEqual(prepared, []);
    assert.ok(released.includes("/old.xml"));
    assert.ok(released.includes("/new.xml"));
    await app.click('[data-action="compare-diff"]');
    assert.deepEqual(prepared, ["/replacement-old.mscz"]);
    assert.deepEqual(exports, [{ id: "/replacement-new.xml", operation: { kind: "diff", older_id: "/replacement-old.mscz" } }]);
    await app.click('[data-action="cancel-diff"]');
    assert.ok(released.includes("/replacement-old.mscz"));
    assert.ok(released.includes("/replacement-new.xml"));
    assert.ok(app.find(".empty"));
  } finally { await app.close(); }
});

for (const entry of ["pair", "inline"] as const) {
  test(`${entry} diff holds selected documents until an in-flight export settles after disposal`, async () => {
    let finish: ((result: { path: string; warnings: string[] }) => void) | undefined;
    const released: string[] = [];
    const app = await mountTestApp((command, args) => {
      if (command === "open_files") return entry === "pair" ? ["/old.xml", "/new.xml"] : ["/new.xml"];
      if (command === "open_file") return "/old.xml";
      if (command === "inspect_file") return score(String(args.path));
      if (command === "export_document") return new Promise<{ path: string; warnings: string[] }>((resolve) => { finish = resolve; });
      if (command === "release_document") { released.push(String(args.id)); return; }
      throw new Error(`Unexpected ${command}`);
    });
    try {
      await app.menu("open-score");
      if (entry === "inline") {
        await app.click('[data-mode="diff"]');
        await app.click('[data-action="open-older"]');
      }
      await app.click(`[data-action="${entry === "pair" ? "compare-diff" : "export"}"]`);
      assert.ok(finish);
      await app.close();
      assert.deepEqual(released, []);
      await act(async () => { finish!({ path: "/diff.xml", warnings: [] }); });
      assert.deepEqual(new Set(released), new Set(["/old.xml", "/new.xml"]));
      assert.equal(released.length, 2);
    } catch (error) { await app.close(); throw error; }
  });
}

test("MuseScore arranging modes prepare MusicXML and dispatch by selected operation", async () => {
  let conversions = 0;
  let fail = false;
  let holdPreparation = false;
  let finishPreparation: ((value: Inspection) => void) | undefined;
  const exports: Record<string, unknown>[] = [];
  const native: Inspection = { ...score("/score.mscz"), format: "musescore", practice_parts: [] };
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return [native.path];
    if (command === "inspect_file") return native;
    if (command === "release_document") return;
    if (command === "practice_environment") return { executable: null, soundfont: null, version: null, error: "Missing", presets: [] };
    if (command === "prepare_musicxml") {
      conversions++;
      if (fail) throw new Error("Conversion failed");
      if (holdPreparation) return new Promise<Inspection>((resolve) => { finishPreparation = resolve; });
      return { ...native, parts: [{ id: "P1", name: "Violin", instrument_names: [], voice_count: 1, split_plans: { chords: [], voices: [] } }] };
    }
    if (command === "export_document") { exports.push(args); return null; }
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.menu("open-score");
    assert.equal(conversions, 0);
    assert.ok(app.find('[data-mode="practice"]'));
    assert.equal(app.document.querySelector(".arrangement-card"), null);
    fail = true;
    await app.click('[data-mode="split"]');
    assert.match(app.find('.status[role="alert"]').textContent!, /Conversion failed/);
    assert.ok(app.find('[data-mode="practice"]'), "failed preparation stays on mode selection");
    assert.equal(app.document.querySelector('[data-action="back-to-modes"]'), null);
    fail = false;
    holdPreparation = true;
    await app.click('[data-mode="split"]');
    for (const tile of app.document.querySelectorAll("[data-mode]"))
      assert.ok((tile as unknown as HTMLButtonElement).disabled, "preparing a mode disables all tiles");
    await app.click('[data-mode="merge"]');
    assert.equal(conversions, 2, "disabled tiles cannot start overlapping preparation");
    assert.equal(app.document.querySelector('[data-action="back-to-modes"]'), null);
    await act(async () => { finishPreparation!({ ...native, parts: [{ id: "P1", name: "Violin", instrument_names: [], voice_count: 1, split_plans: { chords: [], voices: [] } }] }); });
    await app.settle();
    holdPreparation = false;
    await app.click('[data-part="P1"]');
    await app.click('[data-action="export"]');
    assert.equal((exports[0].operation as { kind: string }).kind, "split");
    await app.click('[data-action="back-to-modes"]');
    await app.click('[data-mode="merge"]');
    assert.ok(app.find(".arrangement-card"));
    await app.click('[data-action="back-to-modes"]');
    await app.click('[data-mode="diff"]');
    assert.ok(app.find(".diff-card"));
    await app.click('[data-action="back-to-modes"]');
    assert.equal(app.find<HTMLButtonElement>('[data-mode="practice"]').disabled, false, "conversion retains practice availability");
    await app.click('[data-mode="practice"]');
    assert.ok(app.find('[aria-label="Practice instruments"]'));
  } finally { await app.close(); }
});

test("pair cancellation retries only unreleased documents and Escape restores focus after a native menu action", async () => {
  const released: string[] = [];
  let fail = true;
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return ["/old.xml", "/new.xml"];
    if (command === "inspect_file") return score(String(args.path));
    if (command === "release_document") {
      if (args.id === "/new.xml" && fail) { fail = false; throw new Error("Session busy; retry Cancel"); }
      released.push(String(args.id)); return;
    }
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.menu("open-score");
    await app.click('[data-action="cancel-diff"]');
    assert.deepEqual(released, ["/old.xml"]);
    assert.match(app.find('[role="dialog"]').textContent!, /retry Cancel/);
    assert.ok(app.find<HTMLButtonElement>('[data-action="compare-diff"]').disabled);
    await act(async () => {
      app.document.activeElement!.dispatchEvent(new app.browser.KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    });
    await app.settle();
    assert.deepEqual(released, ["/old.xml", "/new.xml"]);
    assert.equal(app.document.querySelector('[data-action="compare-diff"]'), null);
    assert.equal(app.document.activeElement?.getAttribute("data-action"), "open");
  } finally { await app.close(); }
});

test("two MuseScore files are prepared for direct export and a cancelled pair is released on disposal", async () => {
  const released: string[] = [];
  const prepared: string[] = [];
  const pair = ["/old.mscz", "/new.mscx"];
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return pair;
    if (command === "inspect_file") return { ...score(String(args.path)), format: "musescore", practice_parts: [] };
    if (command === "prepare_musicxml") { prepared.push(String(args.id)); return { ...score(String(args.id)), format: "musescore", practice_parts: [] }; }
    if (command === "release_document") { released.push(String(args.id)); return; }
    if (command === "export_document") return null;
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.menu("open-score");
    await app.click('[data-action="compare-diff"]');
    assert.deepEqual(prepared, pair);
    assert.match(app.find('[role="dialog"]').textContent!, /Export cancelled/);
    assert.equal(app.document.querySelector(".diff-card"), null);
    await app.close();
    assert.deepEqual(new Set(released), new Set(pair));
  } catch (error) { await app.close(); throw error; }
});
