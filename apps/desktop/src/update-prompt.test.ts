import assert from "node:assert/strict";
import test from "node:test";
import { act } from "react";
import { mountTestApp } from "./test-support.ts";
import type { UpdateAction, UpdateWindowState } from "./update-prompt.tsx";

const initial: UpdateWindowState = { revision: 7, currentVersion: "0.2.3", version: "0.2.4-nightly.17", channel: "nightly", status: "available", supported: true, canInstall: true, downgrade: false, error: "" };

test("standalone window uses a labelled shadcn channel select, channel link, typed actions, and remains nonblocking", async () => {
  const actions: UpdateAction[] = [];
  const app = await mountTestApp((command, args) => {
    if (command === "update_prompt_action") { actions.push(args.action as UpdateAction); return; }
    throw new Error(`Unexpected command ${command}`);
  }, { entry: "/src/update-prompt.tsx", initialize(browser) { Object.assign(browser, { __SCOREWING_UPDATE_STATE__: initial }); } });
  try {
    await app.settle();
    assert.equal(actions[0].type, "ready");
    assert.notEqual(app.find('[role="dialog"]').getAttribute("aria-modal"), "true");
    assert.match(app.find('[role="dialog"]').textContent!, /0.2.4-nightly.17.*0.2.3/s);
    assert.equal(app.find("label").getAttribute("for"), "update-channel");
    assert.equal(app.find("a").getAttribute("href"), "https://github.com/Popchor-RUB/Scorewing/releases/tag/nightly");
    await app.choose('[data-action="update-channel"]', "Stable");
    assert.deepEqual(actions.at(-1), { type: "set-channel", channel: "stable", revision: 7 });
    await app.click("a");
    assert.deepEqual(actions.at(-1), { type: "open-downloads", revision: 7 });
    for (const [button, type] of [["skip-update", "skip"], ["download-update", "download"], ["update-later", "cancel"]]) {
      await app.click(`[data-action="${button}"]`);
      assert.equal(actions.at(-1)!.type, type);
    }
    await act(async () => { app.find('[role="dialog"]').dispatchEvent(new app.browser.KeyboardEvent("keydown", { key: "Escape", bubbles: true }) as unknown as Event); });
    assert.equal(actions.at(-1)!.type, "cancel");
  } finally { await app.close(); }
});

test("one window renders checking/current/error/progress/ready/installing/unsupported states and locks unsafe controls", async () => {
  const app = await mountTestApp(() => undefined, { entry: "/src/update-prompt.tsx", initialize(browser) { Object.assign(browser, { __SCOREWING_UPDATE_STATE__: initial }); } });
  const state = async (patch: Partial<UpdateWindowState>) => app.event("update-prompt-state", { ...initial, ...patch });
  try {
    await app.settle();
    await state({ status: "checking", version: undefined });
    assert.match(app.find('[role="status"]').textContent!, /Checking nightly/);
    await state({ status: "idle", version: undefined });
    assert.match(app.find('[role="status"]').textContent!, /up to date/);
    await state({ status: "idle", version: undefined, error: "Offline" });
    assert.match(app.find('[role="alert"]').textContent!, /Offline/);
    assert.ok(app.find('[data-action="retry-update-check"]'));
    await state({ status: "downloading", progress: 30 });
    assert.ok(app.find<HTMLButtonElement>('[data-action="update-channel"]').disabled);
    assert.equal(app.find('[role="progressbar"]').getAttribute("aria-valuenow"), "30");
    await state({ status: "ready", canInstall: false });
    assert.ok(app.find<HTMLButtonElement>('[data-action="install-update"]').disabled);
    assert.match(app.find('[role="dialog"]').textContent!, /Closing a score discards/);
    await state({ status: "installing" });
    assert.ok(app.find<HTMLButtonElement>('[data-action="update-later"]').disabled);
    await state({ status: "idle", version: undefined, supported: false });
    assert.ok(app.find('[data-action="update-downloads"]'));
    assert.match(app.find('[role="dialog"]').textContent!, /DEB\/RPM/);
  } finally { await app.close(); }
});

test("older version offers review, then Back and explicit downgrade confirmation", async () => {
  const actions: UpdateAction[] = [];
  const older = { ...initial, version: "0.2.2", downgrade: true };
  const app = await mountTestApp((command, args) => { if (command === "update_prompt_action") actions.push(args.action as UpdateAction); }, { entry: "/src/update-prompt.tsx", initialize(browser) { Object.assign(browser, { __SCOREWING_UPDATE_STATE__: older }); } });
  try {
    await app.settle();
    assert.equal(app.document.querySelector('[data-action="download-update"]'), null);
    await app.click('[data-action="review-downgrade"]');
    assert.equal(actions.at(-1)!.type, "review-downgrade");
    await app.event("update-prompt-state", { ...older, status: "confirm-downgrade" });
    await app.click('[data-action="back-update"]');
    assert.equal(actions.at(-1)!.type, "back");
    await app.click('[data-action="confirm-downgrade"]');
    assert.equal(actions.at(-1)!.type, "confirm-downgrade");
  } finally { await app.close(); }
});
