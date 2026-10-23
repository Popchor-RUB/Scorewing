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
  let holdExport = false;
  let finishExport: (() => void) | undefined;
  let exportError: string | undefined;
  let exportWarnings: string[] = [];
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
                  voice_count: 3,
                  split_plans: {
                    voices: [
                      { voice: "1", tone: 0, fallback_voice: null },
                      { voice: "2", tone: 0, fallback_voice: null },
                      { voice: "3", tone: 0, fallback_voice: "2" },
                    ],
                    chords: [
                      { voice: "1", tone: 0, fallback_voice: null },
                      { voice: "2", tone: 0, fallback_voice: null },
                      { voice: "2", tone: 1, fallback_voice: null },
                    ],
                  },
                })),
          };
        }
        if (command === "export_document") {
          exported.push(args as unknown as (typeof exported)[number]);
          if (holdExport)
            await new Promise<void>((resolve) => { finishExport = resolve; });
          if (exportError) throw exportError;
          return cancelled ? null : { path: "/scores/arranged.mxl", warnings: exportWarnings };
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
  const choose = async (selector: string, value: string) => {
    const select = document.querySelector(selector) as unknown as HTMLSelectElement;
    assert.ok(select, selector);
    select.value = value;
    select.dispatchEvent(new browser.Event("change", { bubbles: true }) as unknown as Event);
    await settle();
  };
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
  assert.match(document.querySelector(".split-preview")!.textContent, /3 additional staves/);
  const slurToggle = () => document.querySelector("#insert-fallback-slurs") as unknown as HTMLInputElement;
  assert.ok(slurToggle().checked);
  assert.match(document.querySelector("#fallback-connections-hint")!.textContent, /Ties are inserted automatically to continue existing ties when pitches match/);
  assert.match(document.querySelector("#fallback-connections-hint")!.textContent, /When disabled, the fallback note is replaced with a rest of the same duration/);
  assert.equal(slurToggle().getAttribute("aria-describedby"), "fallback-connections-hint");
  assert.match(document.querySelector(".split-preview")!.textContent, /carry compatible voices/);
  assert.match(document.querySelector(".split-preview")!.textContent, /fallback is used only when the staff’s native voices are absent for the whole measure/);
  assert.match(document.querySelector('label[for="fallback-1-2"]')!.textContent, /Violin P2.*Additional staff 3.*Voice 2.*Chord tone 2.*Fallback voice/);
  assert.deepEqual(Array.from((document.querySelector('select[data-lane="2"]') as unknown as HTMLSelectElement).options).map((option) => option.value), ["none", "voice:1", "voice:3"]);
  await choose('select[data-lane="2"]', "voice:3");
  await click('[data-action="export"]');
  assert.deepEqual(exported.at(-1)?.operation, {
    kind: "split", part_ids: ["P2"], mode: "chords",
    insert_fallback_slurs: true,
    fallbacks: [
      { part_id: "P2", voice: "1", tone: 0, fallback_voice: null },
      { part_id: "P2", voice: "2", tone: 0, fallback_voice: null },
      { part_id: "P2", voice: "2", tone: 1, fallback_voice: "3" },
    ],
  });
  slurToggle().focus();
  await click("#insert-fallback-slurs");
  assert.equal(slurToggle().checked, false);
  assert.equal(document.activeElement?.getAttribute("id"), "insert-fallback-slurs");
  await choose("#split-mode", "voices");
  assert.match(document.querySelector(".split-preview")!.textContent, /3 additional staves/);
  const fallbackSelector = 'select[data-fallback-part="P2"][data-lane="2"]';
  const fallbackSelect = () => document.querySelector(fallbackSelector) as unknown as HTMLSelectElement;
  assert.equal(fallbackSelect().value, "voice:2");
  assert.deepEqual(Array.from(fallbackSelect().options).map((option) => [option.value, option.textContent]), [["none", "None (silent)"], ["voice:1", "Voice 1"], ["voice:2", "Voice 2"]]);
  assert.equal((document.querySelector('select[data-lane="0"]') as unknown as HTMLSelectElement).value, "none");
  fallbackSelect().focus();
  await choose(fallbackSelector, "none");
  assert.equal(document.activeElement?.getAttribute("id"), "fallback-1-2");
  await check("P2");
  assert.equal(document.querySelector(".split-preview"), null);
  await check("P2");
  assert.equal(fallbackSelect().value, "none");
  await choose("#split-mode", "chords");
  assert.equal((document.querySelector('select[data-lane="2"]') as unknown as HTMLSelectElement).value, "voice:3");
  await choose("#split-mode", "voices");
  assert.equal(fallbackSelect().value, "none");
  await click('[data-action="select"][data-key="2"]');
  await click('[data-action="select"][data-key="1"]');
  assert.equal(fallbackSelect().value, "none");
  assert.equal(slurToggle().checked, false);
  holdExport = true;
  await click('[data-action="export"]');
  assert.ok(fallbackSelect().disabled);
  assert.ok(slurToggle().disabled);
  finishExport!();
  holdExport = false;
  await settle();
  assert.deepEqual(exported.at(-1)?.operation, {
    kind: "split",
    part_ids: ["P2"],
    mode: "voices",
    fallbacks: ["1", "2", "3"].map((voice) => ({ part_id: "P2", voice, tone: 0, fallback_voice: null })),
    insert_fallback_slurs: false,
  });
  assert.match(document.querySelector(".status")!.textContent, /cancelled/);
  cancelled = false;
  exportWarnings = ['Part "Violin", measure 1, position 0, voice 2, notes [C#4]: ambiguous tie assignment', 'Part "Violin", measure 8, position 1, voice 7, notes [Eb4]: ambiguous slur assignment', 'Part "Violin", measure 2, position 0, voice 1, notes [A4]; from Part "Violin", measure 1, position 0, voice 3, notes [E4]: inserted slur instead of a tie at a voice transition with different pitches'];
  await click('[data-action="export"]');
  assert.equal(document.querySelector(".status")!.textContent, `Exported /scores/arranged.mxl\nReview warnings — affected passages are marked red:\n${exportWarnings.join("\n")}`);
  exportWarnings = [];
  exportError = "Cannot write output file";
  await click('[data-action="export"]');
  assert.equal(document.querySelector('[role="alert"]')!.textContent, exportError);
  assert.equal((document.querySelector('[data-action="export"]') as unknown as HTMLButtonElement).disabled, false);
  exportError = undefined;
  await click('input[name="operation"][value="merge"]');
  assert.equal(document.querySelector("#insert-fallback-slurs"), null);
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
  assert.equal((document.querySelector('select[data-stem-part="P1"]') as unknown as HTMLSelectElement).value, "down");
  await choose('select[data-stem-part="P1"]', "up");
  await choose('select[data-stem-part="P3"]', "down");
  await click('[data-action="up"][data-group="1"][data-index="1"]');
  assert.equal((document.querySelector('select[data-stem-part="P3"]') as unknown as HTMLSelectElement).value, "down");
  cancelled = false;
  holdExport = true;
  await click('[data-action="export"]');
  assert.ok((document.querySelector('select[data-stem-part="P1"]') as unknown as HTMLSelectElement).disabled);
  finishExport!();
  holdExport = false;
  await settle();
  assert.deepEqual(exported.at(-1)?.operation, {
    kind: "merge",
    groups: [
      ["P2", "P1"],
      ["P4", "P3"],
    ],
    stems: [["up", "up"], ["up", "down"]],
  });
  assert.match(document.querySelector(".status")!.textContent, /Exported/);
  await click('[data-action="open"]');
  assert.equal(document.querySelectorAll(".file-row").length, 3);
  await click('[data-action="select"][data-key="3"]');
  await check("P2");
  await choose("#split-mode", "voices");
  assert.equal(fallbackSelect().value, "voice:2", "a new file starts with its own suggestions");
  assert.ok(slurToggle().checked, "a new file defaults to automatic slurs");
  await click('[data-action="select"][data-key="1"]');
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
