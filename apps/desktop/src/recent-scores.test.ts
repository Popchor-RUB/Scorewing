import assert from "node:assert/strict";
import test from "node:test";
import { readRecentScores, RECENT_SCORES_KEY, rememberScore } from "./recent-scores.ts";

test("recent scores persist newest first, deduplicate, limit history, and tolerate invalid or unavailable storage", () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  let stored: string | null = null;
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: {
    getItem(key: string) { assert.equal(key, RECENT_SCORES_KEY); return stored; },
    setItem(key: string, value: string) { assert.equal(key, RECENT_SCORES_KEY); stored = value; },
  } });
  try {
    assert.deepEqual(readRecentScores(), []);
    let recent: string[] = [];
    for (let i = 0; i < 12; i++) recent = rememberScore(recent, `/scores/${i}.xml`);
    assert.deepEqual(readRecentScores(), recent);
    assert.equal(recent.length, 10);
    assert.equal(recent[0], "/scores/11.xml");
    assert.equal(recent[9], "/scores/2.xml");
    recent = rememberScore(recent, "/scores/5.xml");
    assert.equal(recent[0], "/scores/5.xml");
    assert.equal(recent.filter((path) => path === "/scores/5.xml").length, 1);
    assert.deepEqual(readRecentScores(), recent);
    stored = JSON.stringify([null, 1, {}, "", "  ", "/scores/a.xml", "/scores/a.xml", ...recent]);
    assert.deepEqual(readRecentScores(), ["/scores/a.xml", ...recent].slice(0, 10));
    for (const invalid of ["invalid json", "null", "{}", "42"]) {
      stored = invalid;
      assert.deepEqual(readRecentScores(), []);
    }
    Object.defineProperty(globalThis, "localStorage", { configurable: true, get() { throw new Error("Storage blocked"); } });
    assert.deepEqual(readRecentScores(), []);
    recent = rememberScore(recent, "/scores/session.xml");
    assert.equal(recent[0], "/scores/session.xml");
    assert.equal(rememberScore(recent, "/scores/next.xml")[1], "/scores/session.xml");
  } finally {
    if (original) Object.defineProperty(globalThis, "localStorage", original);
    else Reflect.deleteProperty(globalThis, "localStorage");
  }
});
