import assert from "node:assert/strict";
import test from "node:test";
import { act } from "react";
import { mountTestApp, type Invoke } from "./test-support.ts";
import type { Inspection } from "./model.ts";
import type { UpdateAction, UpdateWindowState } from "./update-prompt.tsx";

const metadata = { rid: 10, currentVersion: "0.2.3", version: "0.2.4", body: "A better Scorewing.", rawJson: {}, downgrade: false };
const inspection: Inspection = { id: "score", path: "/scores/test.xml", name: "Test score", format: "musicxml", error: null, parts: [] };

async function mountUpdater(handler: Invoke = () => undefined, options: {
  currentVersion?: string; supported?: boolean; failWindow?: boolean; savedChannel?: string; startupError?: string;
} = {}) {
  let state: UpdateWindowState | undefined;
  let shown = 0;
  let hidden = 0;
  const closed: number[] = [];
  const checks: Record<string, unknown>[] = [];
  const app = await mountTestApp((command, args) => {
    if (command === "update_environment") return { currentVersion: options.currentVersion ?? metadata.currentVersion, supported: options.supported ?? true, startupError: options.startupError };
    if (command === "sync_update_prompt") {
      if (options.failWindow) throw new Error("Window unavailable");
      state = args.state as UpdateWindowState;
      if (args.show) shown++;
      return true;
    }
    if (command === "hide_update_prompt") { hidden++; return; }
    if (command === "plugin:resources|close") { closed.push(Number(args.rid)); return; }
    if (command === "open_files") return [inspection.path];
    if (command === "inspect_file") return inspection;
    if (command === "existing_score_paths") return [];
    if (command === "release_document") return;
    if (command === "check_update") checks.push(args);
    return handler(command, args);
  }, { initialize(browser) { if (options.savedChannel) browser.localStorage.setItem("scorewing.update-channel", options.savedChannel); } });
  await app.settle();
  return { ...app, checks, closed, state: () => { assert.ok(state); return state; }, shown: () => shown, hidden: () => hidden,
    action: (action: UpdateAction) => app.event("update-prompt-action", action) };
}

test("startup checks quietly; every manual check opens a persistent nonblocking window, including errors and current versions", async () => {
  let checks = 0;
  const app = await mountUpdater((command) => {
    if (command === "check_update") { if (++checks < 3) throw new Error("Offline"); return null; }
  });
  try {
    assert.equal(checks, 1);
    assert.equal(app.shown(), 0);
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.deepEqual(app.checks[0], { channel: "stable", manual: false });
    await app.menu("check-updates");
    assert.ok(app.shown() > 0);
    assert.match(app.state().error, /Could not check.*Offline/);
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    assert.equal(app.fileMenu.openEnabled, true);
    await app.action({ type: "check" });
    assert.equal(checks, 3);
    assert.equal(app.state().status, "idle");
    assert.equal(app.state().error, "");
    assert.deepEqual(app.checks.at(-1), { channel: "stable", manual: true });
  } finally { await app.close(); }
});

test("manual checks supersede an in-flight startup check and report only the latest result", async () => {
  const completions: ((value: typeof metadata | null) => void)[] = [];
  const app = await mountUpdater((command) => {
    if (command === "check_update") return new Promise((resolve) => completions.push(resolve));
  });
  try {
    await app.menu("check-updates");
    assert.equal(app.state().status, "checking");
    assert.equal(completions.length, 2);
    await act(async () => completions[1](null));
    assert.equal(app.state().status, "idle");
    await act(async () => completions[0](metadata));
    assert.equal(app.state().version, undefined);
    assert.deepEqual(app.closed, [metadata.rid]);
  } finally { await app.close(); }
});

test("channel follows installed build initially, is saved, and changing it checks immediately and discards stale resources", async () => {
  let finishStable: ((value: typeof metadata) => void) | undefined;
  const app = await mountUpdater((command, args) => {
    if (command === "check_update") {
      if (args.channel === "stable") return new Promise((resolve) => { finishStable = resolve; });
      return { ...metadata, rid: 12, version: "0.2.4-nightly.17" };
    }
  }, { currentVersion: "0.2.4-nightly.16", savedChannel: "stable" });
  try {
    assert.equal(app.checks[0].channel, "stable");
    await app.action({ type: "set-channel", channel: "nightly" });
    assert.equal(app.state().version, "0.2.4-nightly.17");
    assert.equal(app.browser.localStorage.getItem("scorewing.update-channel"), "nightly");
    await act(async () => finishStable!(metadata));
    assert.equal(app.state().channel, "nightly");
    assert.equal(app.state().version, "0.2.4-nightly.17");
    assert.ok(app.closed.includes(metadata.rid));
    await app.remount();
    assert.equal(app.checks.at(-1)!.channel, "nightly");
  } finally { await app.close(); }
  const nightly = await mountUpdater((command) => command === "check_update" ? null : undefined, { currentVersion: "0.2.4-nightly.16" });
  try { assert.equal(nightly.checks[0].channel, "nightly"); }
  finally { await nightly.close(); }
});

test("skip persists per channel, manual checks override it, and newer versions prompt automatically", async () => {
  let version = metadata.version;
  const app = await mountUpdater((command) => command === "check_update" ? { ...metadata, version } : undefined);
  try {
    assert.equal(app.shown(), 1);
    await app.action({ type: "skip" });
    assert.equal(app.hidden(), 1);
    await app.remount();
    assert.equal(app.shown(), 1);
    await app.menu("check-updates");
    assert.ok(app.shown() > 1);
    await app.action({ type: "set-channel", channel: "nightly" });
    await app.action({ type: "skip" });
    assert.equal(app.browser.localStorage.getItem("scorewing.skipped-update-version.nightly"), version);
    assert.equal(app.browser.localStorage.getItem("scorewing.skipped-update-version"), version);
    const before = app.shown();
    version = "0.2.5-nightly.1";
    await app.remount();
    assert.equal(app.shown(), before + 1);
  } finally { await app.close(); }
});

test("native window failures retain a usable shadcn fallback with channel selection and retry", async () => {
  const app = await mountUpdater((command) => command === "check_update" ? metadata : undefined, { failWindow: true });
  try {
    assert.match(app.find('[role="dialog"] [role="alert"]').textContent!, /Window unavailable/);
    assert.ok(app.find('[data-action="download-update"]'));
    await app.choose('[data-action="update-channel"]', "Nightly");
    assert.deepEqual(app.checks.at(-1), { channel: "nightly", manual: true });
  } finally { await app.close(); }
});

test("download consent, progress and installation lock are retained; channel changes cannot race a download or install", async () => {
  let downloads = 0;
  let installs = 0;
  let restarts = 0;
  let finishDownload: (() => void) | undefined;
  let finishInstall: (() => void) | undefined;
  const app = await mountUpdater((command, args) => {
    if (command === "check_update") return metadata;
    if (command === "download_update") {
      downloads++;
      const channel = args.onEvent as { onmessage: (event: unknown) => void };
      channel.onmessage({ event: "Started", data: { contentLength: 100 } });
      channel.onmessage({ event: "Progress", data: { chunkLength: 30 } });
      return new Promise<void>((resolve) => { finishDownload = resolve; });
    }
    if (command === "install_downloaded_update") { installs++; return new Promise<void>((resolve) => { finishInstall = resolve; }); }
    if (command === "plugin:process|restart") { restarts++; return; }
  });
  try {
    assert.equal(downloads, 0);
    await app.action({ type: "download" });
    assert.equal(app.state().status, "downloading");
    assert.equal(app.state().progress, 30);
    await app.action({ type: "set-channel", channel: "nightly" });
    assert.equal(app.state().channel, "stable");
    await app.action({ type: "cancel" });
    assert.equal(app.hidden(), 1);
    await act(async () => finishDownload!());
    assert.equal(installs, 1);
    assert.equal(app.state().status, "installing");
    assert.equal(app.updateMenu.enabled, false);
    assert.equal(app.fileMenu.openEnabled, false);
    await app.action({ type: "cancel" });
    assert.equal(app.hidden(), 1);
    await app.event("tauri://drag-drop", { paths: [inspection.path], position: { x: 0, y: 0 } });
    assert.equal(app.document.querySelector('[data-action="close"]'), null);
    await act(async () => finishInstall!());
    assert.equal(restarts, 1);
  } finally { await app.close(); }
});

test("opening a score before or during download blocks installation, and changing a ready channel disposes bytes", async () => {
  let downloads = 0;
  let installs = 0;
  let finish: (() => void) | undefined;
  const app = await mountUpdater((command) => {
    if (command === "check_update") return metadata;
    if (command === "download_update") { downloads++; return new Promise<void>((resolve) => { finish = resolve; }); }
    if (command === "install_downloaded_update") { installs++; return; }
  });
  try {
    await app.menu("open-score");
    await app.action({ type: "download" });
    assert.equal(downloads, 0);
    assert.equal(app.state().canInstall, false);
    await app.menu("close-score");
    await app.action({ type: "download" });
    await app.menu("open-score");
    await act(async () => finish!());
    assert.equal(installs, 0);
    assert.equal(app.state().status, "ready");
    await app.action({ type: "install" });
    assert.equal(installs, 0);
    await app.action({ type: "set-channel", channel: "nightly" });
    assert.ok(app.closed.includes(metadata.rid));
  } finally { await app.close(); }
});

test("download/install failures retry, and a restart retry never reinstalls consumed bytes", async () => {
  let downloads = 0;
  let installs = 0;
  let restarts = 0;
  const app = await mountUpdater((command) => {
    if (command === "check_update") return metadata;
    if (command === "download_update") { if (++downloads === 1) throw new Error("Network interrupted"); return 11; }
    if (command === "install_downloaded_update") { if (++installs === 1) throw new Error("Disk full"); return; }
    if (command === "plugin:process|restart") { if (++restarts === 1) throw new Error("Restart unavailable"); return; }
  });
  try {
    await app.action({ type: "download" });
    assert.match(app.state().error, /Network interrupted/);
    await app.action({ type: "download" });
    assert.match(app.state().error, /Disk full/);
    await app.action({ type: "install" });
    assert.match(app.state().error, /Restart unavailable/);
    await app.action({ type: "install" });
    assert.equal(downloads, 2);
    assert.equal(installs, 2);
    assert.equal(restarts, 2);
  } finally { await app.close(); }
});

test("older manual channel versions require two explicit steps, and stale downgrade confirmation cannot install another result", async () => {
  let downloads = 0;
  let restarts = 0;
  const app = await mountUpdater((command, args) => {
    if (command === "check_update") return args.manual ? { ...metadata, version: "0.2.2", downgrade: true } : null;
    if (command === "download_update") { downloads++; return 11; }
    if (command === "plugin:process|restart") { restarts++; return; }
  });
  try {
    assert.equal(app.shown(), 0);
    await app.menu("check-updates");
    assert.equal(app.state().downgrade, true);
    await app.action({ type: "download" });
    await app.action({ type: "confirm-downgrade" });
    assert.equal(downloads, 0);
    await app.action({ type: "review-downgrade" });
    assert.equal(app.state().status, "confirm-downgrade");
    const revision = app.state().revision;
    await app.action({ type: "set-channel", channel: "nightly" });
    await app.action({ type: "confirm-downgrade", revision });
    assert.equal(downloads, 0);
    await app.action({ type: "review-downgrade" });
    await app.action({ type: "back" });
    assert.equal(app.state().status, "available");
    await app.action({ type: "review-downgrade" });
    await app.action({ type: "confirm-downgrade" });
    assert.equal(downloads, 1);
    assert.equal(restarts, 1);
  } finally { await app.close(); }
});

test("unsupported installs open the chosen channel's manual downloads without contacting the feed", async () => {
  const opened: unknown[] = [];
  const app = await mountUpdater((command, args) => { if (command === "open_update_downloads") opened.push(args.channel); }, { supported: false });
  try {
    assert.equal(app.checks.length, 0);
    assert.equal(app.shown(), 0);
    await app.menu("check-updates");
    assert.equal(app.state().supported, false);
    await app.action({ type: "set-channel", channel: "nightly" });
    await app.action({ type: "open-downloads" });
    assert.deepEqual(opened, ["nightly"]);
    assert.equal(app.checks.length, 0);
  } finally { await app.close(); }
});

test("unmount releases late resources without opening a window", async () => {
  let finish: ((value: typeof metadata) => void) | undefined;
  const app = await mountUpdater((command) => command === "check_update" ? new Promise((resolve) => { finish = resolve; }) : undefined);
  await app.close();
  finish!(metadata);
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(app.closed, [metadata.rid]);
  assert.equal(app.shown(), 0);
});

test("cancel during a manual check remains dismissed when its result arrives", async () => {
  let complete: ((value: typeof metadata) => void) | undefined;
  const app = await mountUpdater((command, args) => {
    if (command === "check_update") return args.manual ? new Promise((resolve) => { complete = resolve; }) : null;
  });
  try {
    await app.menu("check-updates");
    const shown = app.shown();
    await app.action({ type: "cancel" });
    await act(async () => complete!(metadata));
    assert.equal(app.state().status, "available");
    assert.equal(app.shown(), shown);
    assert.equal(app.hidden(), 1);
  } finally { await app.close(); }
});

test("a recovered native window replaces its fallback and late link errors cannot contaminate another channel", async () => {
  const options = { failWindow: true };
  let failLink: ((error: Error) => void) | undefined;
  const app = await mountUpdater((command) => {
    if (command === "check_update") return metadata;
    if (command === "open_update_downloads") return new Promise((_resolve, reject) => { failLink = reject; });
  }, options);
  try {
    assert.ok(app.find('[role="dialog"]'));
    options.failWindow = false;
    await app.menu("check-updates");
    assert.equal(app.document.querySelector('[role="dialog"]'), null);
    await app.action({ type: "open-downloads" });
    await app.action({ type: "set-channel", channel: "nightly" });
    await act(async () => failLink!(new Error("Late stable failure")));
    assert.equal(app.state().channel, "nightly");
    assert.equal(app.state().error, "");
  } finally { await app.close(); }
});

test("deferred download works with an open score, stays hidden, and manual checks retain the scheduled release", async () => {
  let finish: (() => void) | undefined;
  let installs = 0;
  const app = await mountUpdater((command, args) => {
    if (command === "check_update") return metadata;
    if (command === "download_update") {
      assert.equal(args.deferred, true);
      return new Promise<void>((resolve) => { finish = resolve; });
    }
    if (command === "install_downloaded_update") installs++;
  });
  try {
    await app.menu("open-score");
    const shown = app.shown();
    await app.action({ type: "download-on-next-start" });
    assert.equal(app.state().status, "downloading");
    assert.equal(app.state().deferred, true);
    assert.equal(app.hidden(), 1);
    await app.menu("check-updates");
    assert.equal(app.checks.length, 1);
    await app.action({ type: "cancel" });
    const reopened = app.shown();
    await act(async () => finish!());
    assert.equal(app.state().status, "scheduled");
    assert.equal(app.shown(), reopened);
    assert.ok(reopened > shown);
    assert.equal(installs, 0);
    await app.action({ type: "set-channel", channel: "nightly" });
    await app.menu("check-updates");
    assert.equal(app.state().channel, "stable");
    assert.equal(app.state().status, "scheduled");
    assert.equal(app.checks.length, 1);
  } finally { await app.close(); }
});

test("cancel invalidates late progress and completion; failed cancellation remains retryable", async () => {
  let finish: (() => void) | undefined;
  let progress: ((event: unknown) => void) | undefined;
  let cancellations = 0;
  let installs = 0;
  const app = await mountUpdater((command, args) => {
    if (command === "check_update") return metadata;
    if (command === "download_update") {
      progress = (args.onEvent as { onmessage: (event: unknown) => void }).onmessage;
      return new Promise<void>((resolve) => { finish = resolve; });
    }
    if (command === "cancel_update_download" && ++cancellations === 1) throw new Error("Cleanup failed");
    if (command === "install_downloaded_update") installs++;
  });
  try {
    await app.action({ type: "download-on-next-start" });
    await app.action({ type: "cancel-download" });
    assert.match(app.state().error, /Cleanup failed/);
    assert.equal(app.state().status, "downloading");
    await act(async () => { progress!({ event: "Progress", data: { chunkLength: 30 } }); finish!(); });
    assert.equal(app.state().status, "downloading");
    assert.equal(installs, 0);
    await app.action({ type: "cancel-download" });
    assert.equal(app.state().status, "available");
    assert.equal(app.state().deferred, false);
    assert.equal(app.state().progress, undefined);
    assert.equal(app.state().error, "");
  } finally { await app.close(); }
});

test("background download failures stay hidden and startup signature errors permit manual retry", async () => {
  let fail: ((error: Error) => void) | undefined;
  const app = await mountUpdater((command) => {
    if (command === "check_update") return metadata;
    if (command === "download_update") return new Promise<void>((_resolve, reject) => { fail = reject; });
  }, { startupError: "Invalid signature" });
  try {
    assert.match(app.state().error, /Invalid signature/);
    assert.equal(app.checks.length, 0);
    assert.equal(app.shown(), 0);
    await app.menu("check-updates");
    assert.match(app.state().error, /Invalid signature/);
    await app.action({ type: "check" });
    await app.action({ type: "download-on-next-start" });
    const shown = app.shown();
    await act(async () => fail!(new Error("Network interrupted")));
    assert.match(app.state().error, /Network interrupted/);
    assert.equal(app.state().status, "available");
    assert.equal(app.shown(), shown);
  } finally { await app.close(); }
});

test("successful cancellation suppresses a late immediate download completion", async () => {
  let finish: (() => void) | undefined;
  let installs = 0;
  const app = await mountUpdater((command) => {
    if (command === "check_update") return metadata;
    if (command === "download_update") return new Promise<void>((resolve) => { finish = resolve; });
    if (command === "install_downloaded_update") installs++;
  });
  try {
    await app.action({ type: "download" });
    await app.action({ type: "cancel-download" });
    assert.equal(app.state().status, "available");
    await act(async () => finish!());
    assert.equal(app.state().status, "available");
    assert.equal(installs, 0);
  } finally { await app.close(); }
});
