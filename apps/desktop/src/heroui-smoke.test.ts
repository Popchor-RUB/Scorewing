import assert from "node:assert/strict";
import test from "node:test";
import { Window } from "happy-dom";
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { createServer } from "vite";

test("HeroUI button renders, responds to a press, and includes its styles", async () => {
  const browser = new Window({ url: "http://localhost:1420" });
  const globals = globalThis as unknown as Record<string, unknown>;
  for (const name of ["window", "document", "HTMLElement", "HTMLButtonElement", "SVGElement", "Element", "Node", "Document", "ShadowRoot", "MutationObserver", "getComputedStyle", "CSS"])
    globals[name] = name === "window" ? browser : (browser as unknown as Record<string, unknown>)[name];
  globals.IS_REACT_ACT_ENVIRONMENT = true;
  const server = await createServer({
    server: { middlewareMode: true, hmr: { port: 0 } },
    appType: "custom",
    optimizeDeps: { noDiscovery: true, include: [] },
  });
  const container = browser.document.createElement("div");
  browser.document.body.append(container);
  const root = createRoot(container as unknown as HTMLElement);
  try {
    const { HeroUISmoke } = await server.ssrLoadModule("/src/heroui-smoke.tsx");
    await act(async () => root.render(createElement(HeroUISmoke)));
    const button = container.querySelector("button")!;
    assert.ok(button);
    assert.equal(button.textContent, "Test HeroUI");
    assert.ok(button.classList.contains("button"));
    await act(async () => button.click());
    assert.equal(button.textContent, "HeroUI works");
    const css = await server.transformRequest("/src/style.css");
    assert.ok(css?.code.includes(".button--primary"), "HeroUI styles are compiled by Tailwind");
  } finally {
    try {
      await act(async () => root.unmount());
    } finally {
      await server.close();
      await browser.happyDOM.close();
    }
  }
});
