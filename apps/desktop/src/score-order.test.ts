import assert from "node:assert/strict";
import test from "node:test";
import { orderScorePair, type Inspection } from "./model.ts";

const score = (name: string, modified_ms?: number | null): Inspection => ({ id: name, path: name, name, modified_ms, error: null, parts: [] });

test("old markers precede semantic versions and original modification dates", () => {
  for (const marker of ["OLD", "alt", "outdated"]) {
    const a = score("score-v1.0.0.xml", 1), b = score(`score-${marker}-v2.0.0.xml`, 2);
    assert.deepEqual(orderScorePair(a, b), [b, a]);
  }
  const a = score("old-v2.0.0.xml", 1), b = score("alt-v1.0.0.xml", 2);
  assert.deepEqual(orderScorePair(a, b), [b, a]);
});

test("semantic versions compare numbers, releases, and prerelease identifiers", () => {
  for (const [older, newer] of [
    ["1.9.0", "1.10.0"], ["1.0.0-alpha.2", "1.0.0-alpha.10"],
    ["1.0.0-2", "1.0.0-beta"], ["1.0.0-beta", "1.0.0-beta.1"],
    ["1.0.0-rc.1", "1.0.0"], ["1.0.0+build.2", "2.0.0+build.1"],
  ]) {
    const a = score(`score-v${newer}.mscz`, 1), b = score(`score-v${older}.mxml`, 2);
    assert.deepEqual(orderScorePair(a, b), [b, a], `${older} < ${newer}`);
  }
});

test("equal, missing, or ambiguous versions fall through to timestamps then input order", () => {
  for (const [first, second] of [
    ["a-v1.0.0.xml", "b-v1.0.0.xml"], ["a.xml", "b.xml"],
    ["a-v1.0.0-v2.0.0.xml", "b-v3.0.0.xml"], ["a-v1.0.0-01.xml", "b-v1.0.0.xml"],
  ]) {
    const a = score(first, 200), b = score(second, 100);
    assert.deepEqual(orderScorePair(a, b), [b, a]);
    b.modified_ms = 200;
    assert.deepEqual(orderScorePair(a, b), [a, b]);
    b.modified_ms = null;
    assert.deepEqual(orderScorePair(a, b), [a, b]);
  }
});
