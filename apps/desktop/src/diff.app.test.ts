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
    if (command === "open_file") return path;
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
    result = { path: "/scores/newer-diff.musicxml", warnings: ["Deleted notes: nearest surviving note marked red."] };
    await app.click('[data-action="export"]');
    assert.match(app.find(".status").textContent!, /newer-diff.musicxml.*\nReview warnings:.*\nDeleted notes/);
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
