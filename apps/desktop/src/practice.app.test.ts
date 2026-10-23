import assert from "node:assert/strict";
import test from "node:test";
import { Window } from "happy-dom";
import type { PracticeConfig, PracticeOptions, PracticeResult } from "./model.ts";

test("MuseScore GUI configures independent sounds/groups, retains settings, and reports progress and partial cancellation", async () => {
  const browser = new Window({ url: "http://localhost:1420" });
  const document = browser.document;
  document.body.innerHTML = '<div id="app"></div>';
  const globals = globalThis as unknown as Record<string, unknown>;
  for (const name of ["window", "document", "HTMLElement", "HTMLInputElement", "HTMLSelectElement", "CSS"])
    globals[name] = name === "window" ? browser : name === "document" ? document : (browser as unknown as Record<string, unknown>)[name];
  const callbacks = new Map<number, (...args: unknown[]) => void>();
  const listeners = new Map<string, number>();
  let callbackId = 0;
  let ready = false;
  let finish: ((result: PracticeResult | null) => void) | undefined;
  let exportError: string | null = null;
  let cancelId = "";
  const requests: { id: string; options: PracticeOptions; config: PracticeConfig }[] = [];
  const configs: PracticeConfig[] = [];
  (browser as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback(callback: (...args: unknown[]) => void) { callbacks.set(++callbackId, callback); return callbackId; },
    unregisterCallback(id: number) { callbacks.delete(id); },
    async invoke(command: string, args: Record<string, unknown>) {
      if (command === "plugin:event|listen") { listeners.set(String(args.event), Number(args.handler)); return listeners.size; }
      if (command === "plugin:event|unlisten") return;
      if (command === "open_files") return ["/scores/first.mscz", "/scores/second.mscx", "/scores/old.musicxml"];
      if (command === "inspect_file") {
        const path = String(args.path);
        const isMuseScore = !path.endsWith("musicxml");
        return { id: path, path, name: path.split("/").pop(), error: null, parts: [], format: isMuseScore ? "musescore" : "musicxml",
          practice_parts: isMuseScore ? [
            { key: "one", name: "Violin", visible: true, original_instruments: ["Violin"], existing_preset: { bank: 0, program: 40 } },
            { key: "two", name: "Violin", visible: false, original_instruments: ["Violin", "Viola"], existing_preset: null },
          ] : [],
        };
      }
      if (command === "practice_environment") {
        configs.push(args.config as PracticeConfig);
        return { executable: ready ? "/bin/mscore" : null, soundfont: "/sounds/MS Basic.sf3", version: ready ? "4.7.3" : null,
          presets: [{ bank: 0, program: 40, name: "Violin" }, { bank: 0, program: 0, name: "Piano" }], error: ready ? null : "MuseScore 4.7.x was not found",
        };
      }
      if (command === "pick_practice_path") { ready = true; return "/chosen/mscore"; }
      if (command === "export_practice_tracks") {
        requests.push(args as unknown as (typeof requests)[number]);
        if (exportError) throw exportError;
        return await new Promise<PracticeResult | null>((resolve) => { finish = resolve; });
      }
      if (command === "cancel_practice_export") { cancelId = String(args.id); return; }
      throw new Error(`Unexpected command ${command}`);
    },
  };
  await import("./app.ts");
  const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
  const find = <T extends Element = Element>(selector: string) => { const element = document.querySelector(selector); assert.ok(element, selector); return element as unknown as T; };
  const click = async (selector: string) => { find<HTMLElement>(selector).click(); await settle(); };
  const choose = async (selector: string, value: string) => {
    const input = find<HTMLInputElement>(selector); input.value = value;
    input.dispatchEvent(new browser.Event("change", { bubbles: true }) as unknown as Event); await settle();
  };
  await click('[data-action="open"]');
  assert.equal(find<HTMLInputElement>('[name="operation"][value="practice"]').checked, true);
  assert.equal(find<HTMLInputElement>('[name="operation"][value="split"]').disabled, true);
  assert.match(find("#operation-format-hint").textContent!, /MusicXML/);
  assert.equal(document.querySelectorAll(".practice-part").length, 2);
  assert.match(find(".practice-parts").textContent!, /Hidden in score/);
  assert.equal(find<HTMLInputElement>('[data-practice-index="0"][data-practice-field="create_track"]').checked, true);
  assert.equal(find<HTMLInputElement>('[data-practice-index="1"][data-practice-field="create_track"]').checked, false);
  assert.match(find(".practice-environment").textContent!, /was not found/);
  assert.match(find(".practice-environment").textContent!, /Keep MuseScore closed/);
  await click('[data-action="practice-export"]');
  assert.equal(requests.length, 0);
  await click('[data-action="practice-browse"][data-kind="executable"]');
  assert.equal(configs.at(-1)?.executable, "/chosen/mscore");
  await choose("#foreground_sound-0", "0:0");
  assert.equal(find<HTMLSelectElement>("#foreground_sound-0").value, "0:0", "sound selection persists through immediate render");
  await click('[data-practice-index="0"][data-practice-field="include_in_background"]');
  await click('[data-practice-index="1"][data-practice-field="include_in_background"]');
  await click('[data-action="practice-export"]');
  assert.match(find('[role="alert"]').textContent!, /background sound for instrument 2/);
  await choose("#background_sound-1", "0:40");
  await click("#practice-metronome");
  await choose("#metronome-level", "-12");
  await choose("#background-reduction", "9");
  await click('[data-action="select"][data-key="2"]');
  assert.equal(find<HTMLInputElement>("#background-reduction").value, "6");
  assert.equal(find<HTMLInputElement>("#practice-metronome").checked, false);
  await click('[data-action="select"][data-key="3"]');
  assert.equal(find<HTMLInputElement>('[name="operation"][value="practice"]').disabled, true);
  assert.equal(find<HTMLInputElement>('[name="operation"][value="split"]').disabled, false);
  await click('[data-action="select"][data-key="1"]');
  assert.equal(find<HTMLInputElement>("#background-reduction").value, "9");
  assert.equal(find<HTMLSelectElement>("#foreground_sound-0").value, "0:0");
  await click('[data-action="practice-export"]');
  assert.equal(requests.length, 1);
  assert.deepEqual(requests[0].options, {
    parts: [
      { key: "one", foreground_sound: { bank: 0, program: 0 }, background_sound: { bank: 0, program: 40 }, include_in_background: false, create_track: true },
      { key: "two", foreground_sound: null, background_sound: { bank: 0, program: 40 }, include_in_background: true, create_track: false },
    ], background_reduction_db: 9, metronome_enabled: true, metronome_db: -12,
  });
  assert.equal(find<HTMLSelectElement>("#foreground_sound-0").disabled, true);
  assert.equal(find<HTMLButtonElement>('[data-action="select"][data-key="2"]').disabled, true);
  const progress = callbacks.get(listeners.get("practice-progress")!); assert.ok(progress);
  progress({ payload: { id: "/scores/first.mscz", stage: "Rendering Violin", completed: 1, total: 2 } });
  assert.match(find(".practice-progress").textContent!, /Rendering Violin · 1 \/ 2/);
  await click('[data-action="practice-cancel"]');
  assert.equal(cancelId, "/scores/first.mscz");
  finish!({ directory: "/output/first", completed: [{ key: "one", name: "Violin", path: "/output/first/Violin.mp3" }], error: null, cancelled: true });
  await settle();
  assert.match(find(".practice-results").textContent!, /Rendering cancelled/);
  assert.match(find(".practice-results").textContent!, /Violin.mp3/);
  assert.equal(find<HTMLButtonElement>('[data-action="practice-export"]').disabled, false);
  await click('[data-action="practice-export"]');
  finish!({ directory: "/output/second", completed: [{ key: "one", name: "Violin", path: "/output/second/Violin.mp3" }], error: "Renderer timed out", cancelled: false });
  await settle();
  assert.match(find(".practice-results").textContent!, /Renderer timed out/);
  assert.match(find(".practice-results").textContent!, /1 completed track/);
  await click('[data-action="practice-export"]');
  finish!(null); await settle();
  assert.match(find(".status").textContent!, /Export cancelled/);
  exportError = "Renderer launch failed";
  await click('[data-action="practice-export"]');
  assert.match(find('[role="alert"]').textContent!, /Renderer launch failed/);
  assert.equal(find<HTMLButtonElement>('[data-action="practice-export"]').disabled, false);
  await browser.happyDOM.close();
});
