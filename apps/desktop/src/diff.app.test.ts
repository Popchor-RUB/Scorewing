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

test("diff keeps both selections through cancellation/failure, exports, and releases them on replacement", async () => {
  let path: string | null = "/scores/newer.musicxml";
  const released: string[] = [];
  const requests: Record<string, unknown>[] = [];
  let result: { path: string; warnings: string[] } | null = null;
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
    if (command === "export_document") { requests.push(args); return result; }
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.click('[data-action="open"]');
    await app.click('input[type="radio"][value="diff"]');
    assert.ok(app.find<HTMLButtonElement>('[data-action="export"]').disabled);
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
    result = { path: "/scores/newer-diff.mxl", warnings: ["Deleted notes: nearest surviving note marked red."] };
    await app.click('[data-action="export"]');
    assert.match(app.find(".status").textContent!, /newer-diff.mxl.*\nReview warnings:.*\nDeleted notes/);
    path = "/scores/earlier.xml";
    await app.click('[data-action="open-older"]');
    assert.ok(released.includes("/scores/older.mxl"));
    await app.click('input[type="radio"][value="merge"]');
    await app.click('input[type="radio"][value="diff"]');
    assert.match(app.find(".diff-card").textContent!, /earlier.xml/);
    path = "/scores/another.xml";
    await app.click('[data-action="open"]');
    assert.ok(released.includes("/scores/earlier.xml"));
    assert.ok(released.includes("/scores/newer.musicxml"));
    await app.click('input[type="radio"][value="diff"]');
    assert.ok(app.find<HTMLButtonElement>('[data-action="export"]').disabled);
    path = "/scores/another-old.xml";
    await app.click('[data-action="open-older"]');
    await app.click('[data-action="close"]');
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
    await app.click('[data-action="open"]');
    await app.click('input[type="radio"][value="diff"]');
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
    if (command === "export_document") { exports.push(args); return null; }
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.click('[data-action="open"]');
    paths = ["/new.mscz", "/old.xml"];
    await app.click('[data-action="open"]');
    assert.match(app.find('[data-version="old"]').textContent!, /old.xml/);
    assert.match(app.find('[data-version="new"]').textContent!, /new.mscz/);
    assert.match(app.find(".score-heading").textContent!, /current.xml/);
    await app.click('[data-action="swap-diff"]');
    assert.match(app.find('[data-version="old"]').textContent!, /new.mscz/);
    await app.click('[data-action="compare-diff"]');
    assert.match(app.find('[role="dialog"]').textContent!, /Install MuseScore CLI/);
    assert.equal(released.length, 0);
    fail = false;
    await app.click('[data-action="compare-diff"]');
    assert.equal(app.document.querySelector('[data-action="compare-diff"]'), null);
    assert.match(app.find(".diff-card").textContent!, /Newer versionold.xml.*Older versionnew.mscz/);
    assert.deepEqual(released, ["/current.xml"]);
    await app.click('[data-action="export"]');
    assert.deepEqual(exports[0], { id: "/old.xml", operation: { kind: "diff", older_id: "/new.mscz" } });
    assert.deepEqual(prepared, ["/new.mscz", "/new.mscz"]);
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
    await app.click('[data-action="open"]');
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

test("MuseScore arranging modes prepare MusicXML and dispatch by selected operation", async () => {
  let conversions = 0;
  let fail = false;
  const exports: Record<string, unknown>[] = [];
  const native = { ...score("/score.mscz"), format: "musescore", practice_parts: [] };
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return [native.path];
    if (command === "inspect_file") return native;
    if (command === "release_document") return;
    if (command === "practice_environment") return { executable: null, soundfont: null, version: null, error: "Missing", presets: [] };
    if (command === "prepare_musicxml") {
      conversions++;
      if (fail) throw new Error("Conversion failed");
      return { ...native, parts: [{ id: "P1", name: "Violin", instrument_names: [], voice_count: 1, split_plans: { chords: [], voices: [] } }] };
    }
    if (command === "export_document") { exports.push(args); return null; }
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.click('[data-action="open"]');
    assert.equal(conversions, 0);
    assert.ok(app.find<HTMLInputElement>('input[value="practice"]').checked);
    fail = true;
    await app.click('input[value="split"]');
    assert.match(app.find('.status[role="alert"]').textContent!, /Conversion failed/);
    assert.ok(app.find<HTMLInputElement>('input[value="practice"]').checked);
    fail = false;
    await app.click('input[value="split"]');
    await app.click('[data-part="P1"]');
    await app.click('[data-action="export"]');
    assert.equal((exports[0].operation as { kind: string }).kind, "split");
    await app.click('input[value="merge"]');
    assert.ok(app.find(".arrangement-card"));
    await app.click('input[value="diff"]');
    assert.ok(app.find(".diff-card"));
    await app.click('input[value="practice"]');
    assert.ok(app.find(".practice-parts"));
  } finally { await app.close(); }
});

test("pair cancellation retries only unreleased documents and Escape restores focus", async () => {
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
    await app.click('header [data-action="open"]');
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
    assert.equal(app.document.activeElement, app.find('header [data-action="open"]'));
  } finally { await app.close(); }
});

test("two MuseScore files are both prepared and a pending pair is released on disposal", async () => {
  const released: string[] = [];
  const prepared: string[] = [];
  let pair = ["/old.mscz", "/new.mscx"];
  const app = await mountTestApp((command, args) => {
    if (command === "open_files") return pair;
    if (command === "inspect_file") return { ...score(String(args.path)), format: "musescore", practice_parts: [] };
    if (command === "prepare_musicxml") { prepared.push(String(args.id)); return { ...score(String(args.id)), format: "musescore", practice_parts: [] }; }
    if (command === "release_document") { released.push(String(args.id)); return; }
    throw new Error(`Unexpected ${command}`);
  });
  try {
    await app.click('[data-action="open"]');
    await app.click('[data-action="compare-diff"]');
    assert.deepEqual(prepared, pair);
    assert.ok(app.find<HTMLInputElement>('input[value="diff"]').checked);
    pair = ["/pending-old.mscz", "/pending-new.mscx"];
    await app.click('[data-action="open"]');
    await app.close();
    assert.deepEqual(new Set(released), new Set(["/old.mscz", "/new.mscx", ...pair]));
  } catch (error) { await app.close(); throw error; }
});
