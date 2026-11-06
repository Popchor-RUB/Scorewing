import assert from "node:assert/strict";
import test from "node:test";
import { Window, type HTMLInputElement as TestInput } from "happy-dom";
import { act, createElement, useState } from "react";
import { createServer } from "vite";
import type { PresetId, SoundPreset } from "./model.ts";

const presets: SoundPreset[] = [
  { bank: 0, program: 0, name: "Grand Piano" },
  { bank: 1, program: 0, name: "Grand Piano" },
  { bank: 0, program: 1, name: "Bright Piano" },
  { bank: 0, program: 2, name: "Organ" },
  { bank: 0, program: 3, name: "Violin" },
  { bank: 0, program: 4, name: "Cello" },
  { bank: 0, program: 5, name: "Flute" },
];

test("instrument picker supports search, keyboard selection, recents, and persistence without a clear button", async () => {
  const browser = new Window({ url: "http://localhost:1420" });
  const globals = globalThis as unknown as Record<string, unknown>;
  for (const name of ["window", "document", "navigator", "localStorage", "HTMLElement", "HTMLInputElement", "HTMLTextAreaElement", "HTMLButtonElement", "HTMLDivElement", "HTMLSelectElement", "SVGElement", "Element", "Node", "NodeFilter", "Text", "Document", "ShadowRoot", "MutationObserver", "ResizeObserver", "Event", "CustomEvent", "KeyboardEvent", "MouseEvent", "PointerEvent", "FocusEvent", "getComputedStyle", "CSS", "requestAnimationFrame", "cancelAnimationFrame"])
    Object.defineProperty(globals, name, { configurable: true, writable: true, value: name === "window" ? browser : (browser as unknown as Record<string, unknown>)[name] });
  globals.IS_REACT_ACT_ENVIRONMENT = true;
  // React's development profiler reads React Aria's intentionally unsupported childNodes getter.
  const timeStamp = console.timeStamp;
  Object.defineProperty(console, "timeStamp", { configurable: true, value: undefined });
  const { createRoot } = await import("react-dom/client");
  Object.defineProperty(console, "timeStamp", { configurable: true, value: timeStamp });
  const server = await createServer({ server: { middlewareMode: true, hmr: false, ws: false }, appType: "custom", optimizeDeps: { noDiscovery: true, include: [] } });
  const container = browser.document.createElement("div");
  browser.document.body.append(container);
  const root = createRoot(container as unknown as HTMLElement);
  const commits: Array<PresetId | null> = [];
  const defaults: PresetId[] = [];
  try {
    const { SoundPicker, RecentSoundsProvider } = await server.ssrLoadModule("/src/sound-picker.tsx");
    const { RECENT_SOUNDS_KEY } = await server.ssrLoadModule("/src/recent-sounds.ts");
    function Harness({ disabled = false, catalog = presets }: { disabled?: boolean; catalog?: SoundPreset[] }) {
      const [value, setValue] = useState<PresetId | null>(presets[0]);
      return createElement(RecentSoundsProvider, null,
        createElement(SoundPicker, { label: "Foreground sound", presets: catalog, value, isDisabled: disabled,
          onChange: (next: PresetId | null) => { commits.push(next); setValue(next); },
          onSetDefault: (next: PresetId) => defaults.push(next) }),
        createElement(SoundPicker, { label: "Background sound", presets: catalog, value: null, onChange: () => {} }));
    }
    const render = async (props = {}) => act(async () => root.render(createElement(Harness, props)));
    const input = (index = 0) => container.querySelectorAll<TestInput>('input[role="combobox"]')[index]!;
    const options = () => [...browser.document.querySelectorAll('[role="option"][data-key]')];
    const separators = () => [...browser.document.querySelectorAll('[data-slot="combobox-separator"]')];
    const focus = async (index = 0) => act(async () => { input(index).focus(); });
    const key = async (name: string) => act(async () => { input().dispatchEvent(new browser.KeyboardEvent("keydown", { key: name, code: name, bubbles: true, cancelable: true })); });
    const type = async (text: string) => act(async () => {
      const setter = Object.getOwnPropertyDescriptor(browser.HTMLInputElement.prototype, "value")!.set!;
      setter.call(input(), text);
      input().dispatchEvent(new browser.Event("input", { bubbles: true }));
      input().dispatchEvent(new browser.Event("change", { bubbles: true }));
    });
    const blur = async () => act(async () => { input().blur(); });
    const select = async (name: string) => {
      await focus(); await type(""); await type(name);
      assert.equal(input().value, name);
      assert.ok(options().every((option) => option.textContent.includes(name)), `filtered options for ${name}: ${options().map((option) => option.textContent).join(",")}`);
      await key("ArrowDown"); await key("Enter");
    };

    await render();
    assert.equal(input().value, "Grand Piano");
    assert.equal(input().getAttribute("data-slot"), "combobox-input");
    assert.ok(container.querySelector('[data-slot="combobox-input-group"] > [data-slot="combobox-trigger"] svg'), "standard combobox has an editable input and trailing chevron");
    assert.equal(defaults.length, 0, "choosing a sound does not implicitly change its default");
    await act(async () => { (container.querySelector('[aria-label="Set default foreground sound"]') as unknown as HTMLElement).click(); });
    assert.deepEqual(defaults, [{ bank: 0, program: 0 }], "Set Default explicitly saves the selected sound");
    assert.equal(container.querySelector('[aria-label="Clear foreground sound"]'), null);
    await focus();
    assert.equal(options().length, presets.length, "focus shows the full catalog with an existing selection");
    assert.equal(input().selectionStart, 0);
    assert.equal(input().selectionEnd, "Grand Piano".length);
    assert.equal(options().filter((option) => option.textContent.includes("Grand Piano")).length, 2);
    assert.deepEqual(options().map((option) => option.textContent), presets.map((preset) => preset.name), "options show only instrument names");
    assert.equal(options().filter((option) => option.getAttribute("aria-selected") === "true").length, 1, "selected preset is identified independently of duplicate labels");
    assert.ok(options().find((option) => option.getAttribute("aria-selected") === "true")!.querySelector("svg"), "selected option has a checkmark");
    assert.equal(separators().length, 0, "a single section needs no separator");
    await type("vIo");
    assert.equal(options().length, 1);
    assert.ok(options()[0].textContent.includes("Violin"));
    assert.equal(commits.length, 0, "typing does not commit a selection");
    await key("ArrowDown");
    await key("Enter");
    assert.deepEqual(commits.at(-1), { bank: 0, program: 3 });
    assert.equal(input().value, "Violin");
    assert.deepEqual(JSON.parse(browser.localStorage.getItem(RECENT_SOUNDS_KEY)!), [{ bank: 0, program: 3 }]);
    await blur();
    await focus(1);
    assert.ok(browser.document.body.textContent.includes("Recently selected"), "history is shared across pickers");
    assert.equal(separators().length, 1, "recent and catalog sections have a separator");
    assert.ok(separators()[0].previousElementSibling?.textContent.includes("Recently selected"));
    assert.ok(separators()[0].nextElementSibling?.textContent.includes("All instruments"));
    assert.ok(options()[0].textContent.includes("Violin"));
    assert.equal(options().length, presets.length, "recents are not duplicated in the catalog");
    await act(async () => input(1).blur());
    await focus();
    await type("no such instrument");
    assert.equal(options().length, 0);
    assert.ok(browser.document.body.textContent.includes("No instruments found"));
    assert.ok(!browser.document.body.textContent.includes("Recently selected"));
    assert.equal(separators().length, 0, "empty search results have no separator");
    await key("Escape");
    assert.equal(input().value, "Violin", "Escape restores the committed label");
    await blur();
    await focus();
    await type("Piano");
    const beforeAbandoning = commits.length;
    const historyBeforeAbandoning = browser.localStorage.getItem(RECENT_SOUNDS_KEY);
    await blur();
    assert.equal(input().value, "Violin", "blur abandons an uncommitted query");
    assert.equal(commits.length, beforeAbandoning, "abandoning a query does not commit a sound");
    assert.equal(browser.localStorage.getItem(RECENT_SOUNDS_KEY), historyBeforeAbandoning, "abandoning a query does not change recents");
    await focus();
    await type("Organ");
    await key("ArrowDown");
    await key("Tab");
    await act(async () => input(1).focus());
    assert.equal(input().value, "Violin", "tabbing away abandons a highlighted query");
    assert.equal(commits.length, beforeAbandoning, "Tab does not select the highlighted sound");
    await act(async () => input(1).blur());
    for (const name of ["Bright Piano", "Organ", "Cello", "Flute", "Violin"]) { await select(name); await blur(); }
    const history = JSON.parse(browser.localStorage.getItem(RECENT_SOUNDS_KEY)!);
    assert.equal(history.length, 5);
    assert.deepEqual(history.map((entry: PresetId) => entry.program), [3, 5, 4, 2, 1]);
    await select("Organ");
    await blur();
    assert.deepEqual(JSON.parse(browser.localStorage.getItem(RECENT_SOUNDS_KEY)!).map((entry: PresetId) => entry.program), [2, 3, 5, 4, 1]);
    await focus(1);
    const cello = options().find((option) => option.textContent.includes("Cello"))!;
    await act(async () => { (cello as unknown as HTMLElement).click(); });
    assert.equal(JSON.parse(browser.localStorage.getItem(RECENT_SOUNDS_KEY)!)[0].program, 4, "mouse selection records shared history");
    await act(async () => input(1).blur());
    await select("Organ");
    assert.equal(JSON.parse(browser.localStorage.getItem(RECENT_SOUNDS_KEY)!)[0].program, 2, "explicitly reselecting the current sound moves it first");
    await blur();
    await act(async () => root.render(createElement(Harness, { key: "remount" })));
    await focus();
    assert.ok(options()[0].textContent.includes("Organ"), "recents persist after remount");
    await type("violin");
    assert.ok(!browser.document.body.textContent.includes("Recently selected"));
    assert.equal(separators().length, 0, "searching a single section hides the separator");
    await type("");
    assert.ok(browser.document.body.textContent.includes("Recently selected"), "clearing search restores recents");
    assert.equal(separators().length, 1, "clearing search restores the section separator");
    await blur();
    assert.equal(container.querySelector('[aria-label="Clear foreground sound"]'), null);
    assert.equal(input().value, "Grand Piano");
    await render({ disabled: true });
    assert.equal(input().disabled, true);
    assert.ok(container.querySelector('[data-slot="combobox-trigger"]')!.hasAttribute("disabled"), "disabled picker also disables its trigger");
    assert.ok(container.querySelector('[aria-label="Set default foreground sound"]')!.hasAttribute("disabled"), "disabled picker cannot set a default");
    const defaultsBeforeDisabledClick = defaults.length;
    await act(async () => { (container.querySelector('[aria-label="Set default foreground sound"]') as unknown as HTMLElement).click(); });
    assert.equal(defaults.length, defaultsBeforeDisabledClick);
    await render({ catalog: [] });
    assert.match(container.textContent!, /This sound is unavailable in the current catalog. Choose another instrument\./);
    await focus();
    assert.equal(options().length, 0);
    assert.ok(browser.document.body.textContent.includes("No instruments found"));

    await act(async () => root.render(createElement(Harness, { key: "duplicate-labels" })));
    await focus();
    await type("Grand Piano");
    const secondPiano = options().find((option) => option.getAttribute("data-key") === "1:0")!;
    await act(async () => { (secondPiano as unknown as HTMLElement).click(); });
    assert.deepEqual(commits.at(-1), { bank: 1, program: 0 }, "duplicate names preserve bank and program identity");
    await blur();

    browser.localStorage.setItem(RECENT_SOUNDS_KEY, JSON.stringify([null, { bank: -1, program: 0 }, { bank: 0, program: 256 }, { bank: 0, program: 2 }, { bank: 0, program: 2 }, { bank: 10, program: 10 }]));
    await act(async () => root.render(createElement(Harness, { key: "invalid-history" })));
    await focus();
    assert.ok(options()[0].textContent.includes("Organ"));
    assert.equal(options().length, presets.length, "invalid, duplicate, and unavailable history entries do not add options");
    await blur();
    browser.localStorage.setItem(RECENT_SOUNDS_KEY, "bad json");
    await act(async () => root.render(createElement(Harness, { key: "malformed-history" })));
    await focus();
    assert.ok(!browser.document.body.textContent.includes("Recently selected"));
    await blur();
    Object.defineProperty(globals, "localStorage", { configurable: true, get() { throw new Error("Storage blocked"); } });
    await act(async () => root.render(createElement(Harness, { key: "blocked-history" })));
    await select("Cello");
    await blur();
    await focus();
    assert.ok(options()[0].textContent.includes("Cello"), "storage failure retains in-memory recents");
  } finally {
    await act(async () => root.unmount());
    await server.close();
    await browser.happyDOM.close();
  }
});
