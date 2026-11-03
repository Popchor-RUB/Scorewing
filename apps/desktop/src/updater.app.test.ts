import assert from "node:assert/strict";
import test from "node:test";
import { act } from "react";
import { mountTestApp, type Invoke } from "./test-support.ts";
import type { Inspection } from "./model.ts";

const metadata = { rid: 10, currentVersion: "0.1.0", version: "0.2.0", body: "A better Scorewing.", rawJson: {} };
const inspection: Inspection = { id: "score", path: "/scores/test.xml", name: "Test score", format: "musicxml", error: null, parts: [] };

function native(handler: Invoke = () => undefined, prompt: (args: Record<string, unknown>) => unknown = () => "cancel"): Invoke {
  return (command, args) => {
    if (command === "updater_supported") return true;
    if (command === "show_update_prompt") return prompt(args);
    if (command === "plugin:resources|close") return;
    if (command === "open_files") return [inspection.path];
    if (command === "inspect_file") return inspection;
    if (command === "release_document") return;
    return handler(command, args);
  };
}

test("startup checks quietly; manual failures are visible and retryable", async () => {
  let checks = 0;
  const app = await mountTestApp(native((command, args) => {
    if (command === "plugin:updater|check") {
      checks++;
      assert.equal(args.timeout, 15_000);
      if (checks < 3) throw new Error("Offline");
      return null;
    }
    throw new Error(`Unexpected command ${command}`);
  }));
  try {
    assert.equal(checks, 1);
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.equal(app.document.querySelector('[role="alert"]'), null);
    assert.equal(app.document.querySelector('[data-action="check-updates"]'), null, "updates are opened from the native menu");
    await app.menu("check-updates");
    assert.match(app.find('[role="alert"]').textContent!, /Could not check.*Offline/);
    assert.equal(app.fileMenu.openEnabled, true);
    await app.click('[data-action="retry-update-check"]');
    assert.equal(checks, 3);
    assert.match(app.find('[role="dialog"]').textContent!, /up to date/);
    assert.equal(app.document.querySelector('[role="alert"]'), null);
  } finally { await app.close(); }
});

test("a manual check during the startup check reports its failure", async () => {
  let fail: ((error: Error) => void) | undefined;
  const app = await mountTestApp(native((command) => {
    if (command === "plugin:updater|check") return new Promise((_resolve, reject) => { fail = reject; });
    throw new Error(`Unexpected command ${command}`);
  }));
  try {
    await app.menu("check-updates");
    await act(async () => { fail!(new Error("Timed out")); });
    assert.match(app.find('[role="alert"]').textContent!, /Timed out/);
  } finally { await app.close(); }
});

test("skip persists across launches, manual checks override it, and newer releases prompt again", async () => {
  let version = metadata.version;
  let prompts = 0;
  let choice = "skip";
  const app = await mountTestApp(native((command) => {
    if (command === "plugin:updater|check") return { ...metadata, version };
    throw new Error(`Unexpected command ${command}`);
  }, () => { prompts++; return choice; }));
  try {
    assert.equal(prompts, 1);
    await app.remount();
    assert.equal(prompts, 1, "skipped release stays quiet after restarting");
    choice = "cancel";
    await app.menu("check-updates");
    assert.equal(prompts, 2, "manual checks still offer skipped releases");
    await app.remount();
    assert.equal(prompts, 2, "cancel leaves the saved skip unchanged");
    version = "0.3.0";
    await app.remount();
    assert.equal(prompts, 3, "the next release is offered automatically");
    await app.remount();
    assert.equal(prompts, 4, "cancel allows the reminder on the next launch");
  } finally { await app.close(); }
});

test("the native prompt locks score operations and failures offer a usable fallback", async () => {
  let answer: ((choice: string) => void) | undefined;
  let fail = false;
  const app = await mountTestApp(native((command) => {
    if (command === "plugin:updater|check") return metadata;
    throw new Error(`Unexpected command ${command}`);
  }, () => {
    if (fail) throw new Error("Window unavailable");
    return new Promise<string>((resolve) => { answer = resolve; });
  }));
  try {
    assert.equal(app.fileMenu.openEnabled, false);
    await app.event("tauri://drag-drop", { paths: [inspection.path], position: { x: 0, y: 0 } });
    assert.equal(app.document.querySelector('[data-action="close"]'), null);
    await act(async () => { answer!("cancel"); });
    assert.equal(app.fileMenu.openEnabled, true);
    fail = true;
    await app.menu("check-updates");
    assert.match(app.find('[role="dialog"] [role="alert"]').textContent!, /Window unavailable/);
    assert.equal(app.fileMenu.openEnabled, true);
    assert.ok(app.find('[data-action="skip-update"]'));
  } finally { await app.close(); }
});

test("startup prompts natively; download and install require consent and lock file operations", async () => {
  let downloads = 0;
  let installs = 0;
  let restarts = 0;
  let finishDownload: ((rid: number) => void) | undefined;
  let finishInstall: (() => void) | undefined;
  let choice = "cancel";
  const prompts: Record<string, unknown>[] = [];
  const app = await mountTestApp(native((command, args) => {
    if (command === "plugin:updater|check") return metadata;
    if (command === "plugin:updater|download") {
      downloads++;
      assert.equal(args.rid, metadata.rid);
      const channel = args.onEvent as { onmessage: (event: unknown) => void };
      channel.onmessage({ event: "Started", data: { contentLength: 100 } });
      channel.onmessage({ event: "Progress", data: { chunkLength: 30 } });
      return new Promise<number>((resolve) => { finishDownload = resolve; });
    }
    if (command === "plugin:updater|install") {
      installs++;
      assert.equal(args.updateRid, metadata.rid);
      assert.equal(args.bytesRid, 11);
      return new Promise<void>((resolve) => { finishInstall = resolve; });
    }
    if (command === "plugin:process|restart") { restarts++; return; }
    throw new Error(`Unexpected command ${command}`);
  }, (args) => { prompts.push(args); return choice; }));
  try {
    assert.match(app.updateMenu.text, /Update available/);
    assert.deepEqual(prompts, [{ currentVersion: "0.1.0", version: "0.2.0", canInstall: true }]);
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.equal(downloads, 0);
    await app.menu("open-score");
    await app.menu("check-updates");
    assert.equal(prompts.at(-1)!.canInstall, false);
    await app.menu("close-score");
    choice = "install";
    await app.menu("check-updates");
    assert.equal(downloads, 1);
    assert.equal(app.find('[role="progressbar"]').getAttribute("aria-valuenow"), "30");
    await act(async () => { finishDownload!(11); });
    assert.equal(installs, 1);
    assert.equal(app.updateMenu.enabled, false);
    assert.match(app.updateMenu.text, /Installing/);
    assert.ok(!app.fileMenu.openEnabled);
    assert.ok(app.find<HTMLButtonElement>('[data-action="update-later"]').disabled);
    await app.event("tauri://drag-drop", { paths: [inspection.path], position: { x: 0, y: 0 } });
    assert.equal(app.document.querySelector('[data-action="close"]'), null);
    await act(async () => { finishInstall!(); });
    assert.equal(restarts, 1);
    assert.ok(!app.fileMenu.openEnabled, "stay locked until the process exits");
  } finally { await app.close(); }
});

test("download and install failures can be retried; a restart failure never reinstalls", async () => {
  let downloads = 0;
  let installs = 0;
  let restarts = 0;
  const app = await mountTestApp(native((command) => {
    if (command === "plugin:updater|check") return metadata;
    if (command === "plugin:updater|download") {
      if (++downloads === 1) throw new Error("Network interrupted");
      return 11;
    }
    if (command === "plugin:updater|install") {
      if (++installs === 1) throw new Error("Disk is full");
      return;
    }
    if (command === "plugin:process|restart") {
      if (++restarts === 1) throw new Error("Restart unavailable");
      return;
    }
    throw new Error(`Unexpected command ${command}`);
  }, () => "install"));
  try {
    assert.match(app.find('[role="alert"]').textContent!, /Network interrupted/);
    await app.click('[data-action="download-update"]');
    assert.match(app.find('[role="alert"]').textContent!, /Disk is full/);
    assert.equal(app.fileMenu.openEnabled, true);
    await app.click('[data-action="install-update"]');
    assert.match(app.find('[role="alert"]').textContent!, /Restart unavailable/);
    await app.click('[data-action="install-update"]');
    assert.equal(downloads, 2);
    assert.equal(installs, 2, "retrying restart must not reuse consumed updater bytes");
    assert.equal(restarts, 2);
  } finally { await app.close(); }
});

test("opening a score during download prevents automatic installation", async () => {
  let finishDownload: ((rid: number) => void) | undefined;
  let installs = 0;
  const app = await mountTestApp(native((command) => {
    if (command === "plugin:updater|check") return metadata;
    if (command === "plugin:updater|download") return new Promise<number>((resolve) => { finishDownload = resolve; });
    if (command === "plugin:updater|install") { installs++; return; }
    throw new Error(`Unexpected command ${command}`);
  }, () => "install"));
  try {
    await app.click('[data-action="update-later"]');
    await app.menu("open-score");
    await act(async () => { finishDownload!(11); });
    assert.equal(installs, 0);
    await app.menu("check-updates");
    assert.ok(app.find<HTMLButtonElement>('[data-action="install-update"]').disabled);
    assert.match(app.find('[role="dialog"]').textContent!, /Close.*score/);
  } finally { await app.close(); }
});

test("DEB/RPM users get manual downloads without contacting the updater feed", async () => {
  let opened = 0;
  const app = await mountTestApp((command) => {
    if (command === "updater_supported") return false;
    if (command === "open_update_downloads") { opened++; return; }
    throw new Error(`Unexpected command ${command}`);
  });
  try {
    await app.menu("check-updates");
    assert.match(app.find('[role="dialog"]').textContent!, /AppImage.*DEB\/RPM/);
    await app.click('[data-action="update-downloads"]');
    assert.equal(opened, 1);
    assert.equal(app.document.querySelector('[role="alert"]'), null);
  } finally { await app.close(); }
});

test("unmount releases a late update resource without opening a dialog", async () => {
  let finish: ((result: typeof metadata) => void) | undefined;
  const released: number[] = [];
  const app = await mountTestApp((command, args) => {
    if (command === "updater_supported") return true;
    if (command === "plugin:updater|check") return new Promise((resolve) => { finish = resolve; });
    if (command === "plugin:resources|close") { released.push(Number(args.rid)); return; }
    throw new Error(`Unexpected command ${command}`);
  });
  await app.close();
  finish!(metadata);
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(released, [metadata.rid]);
});
