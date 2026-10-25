import assert from "node:assert/strict";
import test from "node:test";
import { advancePracticeProgress as advance } from "./practice-progress.ts";

test("progress estimates parallel exports, caps at active work and jumps for completed tracks", () => {
  assert.equal(advance(0, 0, 0, 0, 1000), 0);
  assert.equal(advance(0, 0, 0, 10, 1000), 0);
  assert.equal(advance(20, 2, 3, 10, 1000), 23);
  assert.equal(advance(20, 2, 3, 10, 20_000), 50);
  assert.equal(advance(50, 2, 3, 10, 1000), 50);
  assert.equal(advance(50, 3, 3, 10, 1000), 53, "time at the cap is discarded");
  assert.equal(advance(10, 4, 2, 10, 0), 40, "completion jumps forward immediately");
  assert.equal(advance(55, 4, 1, 10, 1000), 55, "a lower cap never moves progress back");
  assert.equal(advance(70, 7, 1, 10, 1000), 71, "speed falls with active count");
  assert.equal(advance(0, 0, 1, 1, 10_000), 99);
  assert.equal(advance(99, 1, 0, 1, 0), 99, "100% waits for successful export result");
});
