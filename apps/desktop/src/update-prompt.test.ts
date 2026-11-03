import assert from "node:assert/strict";
import test from "node:test";
import { act } from "react";
import { mountTestApp } from "./test-support.ts";

test("native prompt shows the icon, versions, changelog, three choices and Escape", async () => {
  const choices: unknown[] = [];
  let opened = 0;
  let canInstall = true;
  const app = await mountTestApp((command, args) => {
    if (command === "plugin:event|emit_to") {
      assert.equal(args.event, "update-prompt-choice");
      choices.push(args.payload);
      return;
    }
    if (command === "open_update_downloads") { opened++; return; }
    throw new Error(`Unexpected command ${command}`);
  }, {
    entry: "/src/update-prompt.tsx",
    initialize(browser) {
      Object.assign(browser, { __SCOREWING_UPDATE_PROMPT__: { currentVersion: "0.1.0", version: "0.2.0", get canInstall() { return canInstall; } } });
    },
  });
  try {
    assert.match(app.find('[role="dialog"]').textContent!, /0.2.0.*0.1.0/s);
    assert.ok(app.find('img[alt="Scorewing"]').getAttribute("src"));
    assert.deepEqual(Array.from(app.document.querySelectorAll("button")).map((button) => button.textContent), ["Cancel", "Skip this version", "Download & Install"]);
    assert.equal(app.find("a").getAttribute("href"), "https://github.com/Popchor-RUB/Scorewing/releases/latest");
    await app.click("a");
    assert.equal(opened, 1);
    for (const action of ["update-later", "skip-update", "download-update"]) {
      await app.click(`[data-action="${action}"]`);
      await app.remount();
    }
    assert.deepEqual(choices, ["cancel", "skip", "install"]);
    await act(async () => {
      app.find('[role="dialog"]').dispatchEvent(new app.browser.KeyboardEvent("keydown", { key: "Escape", bubbles: true }) as unknown as Event);
    });
    assert.equal(choices.at(-1), "cancel");
    canInstall = false;
    await app.remount();
    assert.ok(app.find<HTMLButtonElement>('[data-action="download-update"]').disabled);
    assert.match(app.find('[role="dialog"]').textContent!, /Close any open score/);
  } finally { await app.close(); }
});
