import assert from "node:assert/strict";
import test from "node:test";
import { Window } from "happy-dom";
import type { Operation } from "./model.ts";

test("desktop queue, errors, grouping, splitting, cancellation, and export", async () => {
  const browser = new Window({ url: "http://localhost:1420" });
  const document = browser.document;
  document.body.innerHTML = '<div id="app"></div>';
  const globals = globalThis as unknown as Record<string, unknown>;
  for (const name of [
    "window",
    "document",
    "HTMLElement",
    "HTMLInputElement",
    "HTMLSelectElement",
    "CSS",
  ]) {
    globals[name] =
      name === "window"
        ? browser
        : name === "document"
          ? document
          : (browser as unknown as Record<string, unknown>)[name];
  }
  const callbacks = new Map<number, (...args: unknown[]) => void>();
  const listeners = new Map<string, number>();
  let callbackId = 0;
  let openCount = 0;
  let cancelled = true;
  let exportError: string | undefined;
  const exported: { id: string; operation: Operation }[] = [];
  const pending: { finish?: () => void } = {};
  const released: string[] = [];
  (browser as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ =
    {
      metadata: {
        currentWindow: { label: "main" },
        currentWebview: { label: "main" },
      },
      transformCallback(callback: (...args: unknown[]) => void) {
        callbacks.set(++callbackId, callback);
        return callbackId;
      },
      unregisterCallback(id: number) {
        callbacks.delete(id);
      },
      async invoke(command: string, args: Record<string, unknown>) {
        if (command === "plugin:event|listen") {
          listeners.set(String(args.event), Number(args.handler));
          return listeners.size;
        }
        if (command === "plugin:event|unlisten") return;
        if (command === "open_files")
          return openCount++
            ? ["/scores/arranged.mxl"]
            : ["/scores/orchestra.xml", "/scores/broken.xml"];
        if (command === "release_document") {
          released.push(String(args.id));
          return;
        }
        if (command === "inspect_file") {
          const path = String(args.path);
          const broken = path.includes("broken");
          if (broken)
            await new Promise<void>((resolve) => {
              pending.finish = resolve;
            });
          return {
            id: path,
            path,
            name: path.split("/").pop(),
            error: broken ? "Part P2: invalid duration" : null,
            parts: broken
              ? []
              : ["P1", "P2", "P3", "P4"].map((id) => ({
                  id,
                  name: "Violin",
                  instrument_names: ["Violin"],
                  voice_count: 2,
                })),
          };
        }
        if (command === "export_document") {
          exported.push(args as unknown as (typeof exported)[number]);
          if (exportError) throw exportError;
          return cancelled ? null : "/scores/arranged.mxl";
        }
        throw new Error(`Unexpected command ${command}`);
      },
    };
  await import("./app.ts");
  const settle = async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  };
  const click = async (selector: string) => {
    const element = document.querySelector(selector) as unknown as {
      click(): void;
    } | null;
    assert.ok(element, selector);
    element.click();
    await settle();
  };
  const check = async (id: string) => click(`input[data-part="${id}"]`);
  await click('[data-action="open"]');
  assert.equal(document.querySelectorAll(".file-row").length, 2);
  assert.equal(document.querySelectorAll(".instrument").length, 4);
  assert.ok(
    (
      document.querySelector(
        '[data-action="export"]',
      ) as unknown as HTMLButtonElement
    ).disabled,
  );
  assert.match(
    document.querySelector(".export-bar")!.textContent,
    /Wait for queued scores/,
  );
  pending.finish!();
  await settle();
  assert.match(
    document.querySelector(".file-list")!.textContent,
    /Could not read score/,
  );
  document.querySelector(".editor")!.scrollTop = 190;
  document.querySelector(".file-list")!.scrollTop = 70;
  browser.scrollTo(0, 230);
  await check("P1");
  assert.equal(document.querySelector(".editor")!.scrollTop, 190);
  assert.equal(document.querySelector(".file-list")!.scrollTop, 70);
  assert.equal(browser.scrollY, 230);
  await click('input[name="operation"][value="merge"]');
  assert.equal(document.querySelector(".editor")!.scrollTop, 190);
  assert.equal(document.querySelector(".file-list")!.scrollTop, 70);
  assert.equal(browser.scrollY, 230);
  await click('[data-action="select"][data-key="2"]');
  assert.equal(document.querySelector(".editor")!.scrollTop, 0);
  assert.equal(document.querySelector(".file-list")!.scrollTop, 70);
  assert.equal(browser.scrollY, 0);
  assert.match(
    document.querySelector(".editor")!.textContent,
    /Part P2: invalid duration/,
  );
  await click('[data-action="select"][data-key="1"]');
  await click('input[name="operation"][value="split"]');
  await click('[data-action="export"]');
  assert.match(
    document.querySelector('[role="alert"]')!.textContent,
    /Select at least one/,
  );
  assert.equal(exported.length, 0);
  await check("P2");
  await click('[data-action="export"]');
  assert.deepEqual(exported.at(-1)?.operation, {
    kind: "split",
    part_ids: ["P2"],
    mode: "chords",
  });
  assert.match(document.querySelector(".status")!.textContent, /cancelled/);
  exportError = 'Part "Violin", measure 1, position 0, voice 2, notes [C#4]: ambiguous tie assignment\nPart "Violin", measure 8, position 1, voice 7, notes [Eb4]: ambiguous slur assignment';
  await click('[data-action="export"]');
  assert.equal(document.querySelector('[role="alert"]')!.textContent, exportError);
  assert.equal((document.querySelector('[data-action="export"]') as unknown as HTMLButtonElement).disabled, false);
  exportError = undefined;
  await click('input[name="operation"][value="merge"]');
  await check("P1");
  await check("P2");
  await click('[data-action="add-group"]');
  assert.ok(
    (
      document.querySelector(
        'input[data-part="P1"]',
      ) as unknown as HTMLInputElement
    ).disabled,
  );
  await click('[data-action="down"][data-index="0"]');
  await check("P3");
  await check("P4");
  await click('[data-action="add-group"]');
  assert.equal(document.querySelectorAll(".merge-group").length, 2);
  cancelled = false;
  await click('[data-action="export"]');
  assert.deepEqual(exported.at(-1)?.operation, {
    kind: "merge",
    groups: [
      ["P2", "P1"],
      ["P3", "P4"],
    ],
  });
  assert.match(document.querySelector(".status")!.textContent, /Exported/);
  await click('[data-action="open"]');
  assert.equal(document.querySelectorAll(".file-row").length, 3);
  await click('[data-action="remove"][data-key="2"]');
  assert.deepEqual(released, ["/scores/broken.xml"]);
  assert.equal(document.querySelectorAll(".file-row").length, 2);
  const drop = callbacks.get(listeners.get("tauri://drag-drop")!);
  assert.ok(drop, "native drag-drop listener registered");
  drop({
    payload: {
      paths: ["/scores/dropped.musicxml"],
      position: { x: 20, y: 20 },
    },
  });
  await settle();
  assert.equal(document.querySelectorAll(".file-row").length, 3);
  drop({
    payload: {
      paths: ["/scores/dropped.musicxml"],
      position: { x: 20, y: 20 },
    },
  });
  await settle();
  assert.equal(document.querySelectorAll(".file-row").length, 3);
  await browser.happyDOM.close();
});
