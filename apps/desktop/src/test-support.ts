import assert from "node:assert/strict";
import { Window } from "happy-dom";
import { act } from "react";
import { createServer } from "vite";

export type Invoke = (command: string, args: Record<string, unknown>) => unknown | Promise<unknown>;

export async function mountTestApp(invoke: Invoke) {
  const browser = new Window({ url: "http://localhost:1420" });
  const document = browser.document;
  document.body.innerHTML = '<div id="app"></div>';
  const globals = globalThis as unknown as Record<string, unknown>;
  for (const name of [
    "window", "document", "navigator", "HTMLElement", "HTMLButtonElement", "HTMLInputElement", "HTMLSelectElement", "HTMLTextAreaElement",
    "SVGElement", "Element", "Node", "NodeFilter", "Document", "DocumentFragment", "ShadowRoot", "MutationObserver", "ResizeObserver",
    "Event", "InputEvent", "MouseEvent", "KeyboardEvent", "PointerEvent", "FocusEvent", "CustomEvent", "getComputedStyle", "CSS", "localStorage",
  ]) Object.defineProperty(globals, name, { configurable: true, writable: true, value: name === "window" ? browser : (browser as unknown as Record<string, unknown>)[name] });
  globals.requestAnimationFrame = browser.requestAnimationFrame.bind(browser);
  globals.cancelAnimationFrame = browser.cancelAnimationFrame.bind(browser);
  globals.IS_REACT_ACT_ENVIRONMENT = true;
  const callbacks = new Map<number, (...args: unknown[]) => void>();
  const listeners = new Map<string, number>();
  const registrations: string[] = [];
  const unregistered: number[] = [];
  let callbackId = 0;
  (browser as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    transformCallback(callback: (...args: unknown[]) => void) { callbacks.set(++callbackId, callback); return callbackId; },
    unregisterCallback(id: number) { callbacks.delete(id); },
    async invoke(command: string, args: Record<string, unknown> = {}) {
      if (command === "plugin:event|listen") {
        registrations.push(String(args.event));
        listeners.set(String(args.event), Number(args.handler));
        return registrations.length;
      }
      if (command === "plugin:event|unlisten") { unregistered.push(Number(args.eventId)); return; }
      return invoke(command, args);
    },
  };
  (browser as unknown as { __TAURI_EVENT_PLUGIN_INTERNALS__: unknown }).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener() {},
  };
  const server = await createServer({
    server: { middlewareMode: true, hmr: false }, appType: "custom",
    optimizeDeps: { noDiscovery: true, include: [] },
  });
  // Keep React's development profiler from inspecting React Aria collection getters.
  const timeStamp = console.timeStamp;
  Object.defineProperty(console, "timeStamp", { configurable: true, value: undefined });
  let mountApp: (container: HTMLElement) => () => void;
  try { ({ mountApp } = await server.ssrLoadModule("/src/app.tsx")); }
  finally { Object.defineProperty(console, "timeStamp", { configurable: true, value: timeStamp }); }
  let dispose: (() => void) | undefined;
  await act(async () => { dispose = mountApp(document.querySelector("#app") as unknown as HTMLElement); });
  const settle = async () => { await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); }); };
  const find = <T extends Element = HTMLElement>(selector: string) => {
    const element = document.querySelector(selector);
    assert.ok(element, `Missing ${selector}`);
    return element as unknown as T;
  };
  const click = async (selector: string) => {
    await act(async () => {
      const element = find<HTMLElement>(selector);
      const control = element.matches('[data-slot="checkbox"]') ? element.querySelector("input") as unknown as HTMLElement : element;
      control.focus();
      control.click();
    });
    await settle();
  };
  const input = async (selector: string, value: string) => {
    await act(async () => {
      const field = find<HTMLInputElement>(selector);
      field.focus();
      const setter = Object.getOwnPropertyDescriptor(browser.HTMLInputElement.prototype, "value")!.set!;
      setter.call(field, value);
      field.dispatchEvent(new browser.Event("input", { bubbles: true }) as unknown as Event);
      field.dispatchEvent(new browser.Event("change", { bubbles: true }) as unknown as Event);
    });
    await settle();
    await act(async () => { find<HTMLInputElement>(selector).blur(); });
    await settle();
  };
  const choose = async (selector: string, text: string) => {
    await click(selector);
    const controlled = find(selector).getAttribute("aria-controls");
    const list = controlled ? document.getElementById(controlled) : document;
    const options = Array.from((list ?? document).querySelectorAll('[role="option"]')).filter((element) => !element.closest('[aria-hidden="true"], [data-exiting]'));
    const option = options.find((element) => element.textContent?.trim() === text)
      ?? options.find((element) => element.textContent?.includes(text));
    assert.ok(option, `Missing option ${text}; available: ${options.map((option) => option.textContent).join(", ")}`);
    await act(async () => { (option as unknown as HTMLElement).click(); });
    await settle();
  };
  const event = async (name: string, payload: unknown) => {
    const callback = callbacks.get(listeners.get(name)!);
    assert.ok(callback, `Missing listener ${name}`);
    await act(async () => callback({ payload }));
    await settle();
  };
  return {
    browser, document, find, click, input, choose, settle, event, registrations, unregistered, server,
    async close() {
      try { await act(async () => { dispose?.(); }); }
      finally { await server.close(); await browser.happyDOM.close(); }
    },
  };
}
