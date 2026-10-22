import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  addGroup,
  fallbackVoice,
  movePart,
  newArrangement,
  operation,
  setFallbackVoice,
  type Arrangement,
  type Inspection,
  type Part,
} from "./model.ts";

interface QueueItem {
  key: number;
  path: string;
  name: string;
  status: "loading" | "ready" | "error";
  document?: Inspection;
  error?: string;
  arrangement: Arrangement;
}

const root = document.querySelector<HTMLDivElement>("#app")!;
const queue: QueueItem[] = [];
let nextKey = 1;
let selectedKey: number | null = null;
let renderedKey: number | null = null;
let busy = false;
let message = "";
let messageError = false;
let dropping = false;

function escape(value: string): string {
  return value.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ]!,
  );
}

function selected(): QueueItem | undefined {
  return queue.find((item) => item.key === selectedKey);
}

function notify(text: string, error = false): void {
  message = text;
  messageError = error;
  render();
}

function splitPreview(part: Part, index: number, state: Arrangement): string {
  if (state.kind !== "split" || !state.selected.has(part.id)) return "";
  const lanes = part.split_plans[state.mode];
  const voices = [...new Set([...part.split_plans.voices, ...lanes].map((lane) => lane.voice))];
  return `<section class="split-preview" aria-label="Split preview for ${escape(part.name || part.id)} ${escape(part.id)}">
    <p>${lanes.length} additional ${lanes.length === 1 ? "staff" : "staves"} · Original staff retained</p>
    ${state.mode === "chords" ? "<p>Chord-tone staffs may also carry compatible voices in measures without that chord tone.</p>" : ""}
    <p>A fallback is used only when the staff’s native voices are absent for the whole measure.</p>
    ${lanes.map((lane, li) => {
      const label = `Additional staff ${li + 1} · Voice ${lane.voice}${state.mode === "chords" ? ` · Chord tone ${lane.tone + 1}` : ""}`;
      return `<div class="fallback-row"><label for="fallback-${index}-${li}"><span class="sr-only">${escape(part.name || part.id)} ${escape(part.id)} · </span>${escape(label)} · Fallback voice</label><select id="fallback-${index}-${li}" data-fallback-part="${escape(part.id)}" data-lane="${li}" data-focus="fallback-${index}-${li}" ${busy ? "disabled" : ""}><option value="none">None (silent)</option>${voices.filter((voice) => voice !== lane.voice).map((voice) => `<option value="voice:${escape(voice)}">Voice ${escape(voice)}</option>`).join("")}</select></div>`;
    }).join("")}
  </section>`;
}

function render(): void {
  const focus = (document.activeElement as HTMLElement | null)?.dataset.focus;
  const sameScore = renderedKey === selectedKey;
  const editorTop = sameScore
    ? (root.querySelector(".editor")?.scrollTop ?? 0)
    : 0;
  const filesTop = root.querySelector(".file-list")?.scrollTop ?? 0;
  const pageTop = sameScore ? window.scrollY : 0;
  const pageLeft = window.scrollX;
  const item = selected();
  const score = item?.document;
  const state = item?.arrangement;
  const ready = item?.status === "ready" && score && state;
  const loading = queue.some((entry) => entry.status === "loading");
  const assigned = new Set(state?.groups.flat() ?? []);
  root.innerHTML = `
    <header class="header"><div class="brand"><span class="brand-icon" aria-hidden="true">♫</span><div><strong>Voiceweaver</strong><span class="tagline">A little room to rearrange.</span></div></div><button type="button" class="primary" data-action="open" data-focus="open" ${busy ? "disabled" : ""}>Open files<span aria-hidden="true"> ↗</span></button></header>
    <main class="workspace ${dropping ? "dragging" : ""}">
      <aside class="files" aria-label="Loaded scores"><div class="section-heading"><h1>Your scores</h1><span class="count">${queue.length}</span></div><p class="hint">Drop MusicXML files anywhere in this window.</p>
      <div class="file-list">${queue.map((file) => `<div class="file-row ${file.key === selectedKey ? "active" : ""}"><button type="button" class="file-choice" data-action="select" data-key="${file.key}" data-focus="file-${file.key}" aria-pressed="${file.key === selectedKey}" ${busy ? "disabled" : ""}><span class="file-icon" aria-hidden="true">♩</span><span><strong>${escape(file.name)}</strong><small class="${file.status === "error" ? "error-text" : ""}">${file.status === "loading" ? "Reading score…" : file.status === "error" ? "Could not read score" : `${file.document!.parts.length} instruments`}</small></span></button><button type="button" class="remove" aria-label="Remove ${escape(file.name)}" data-action="remove" data-key="${file.key}" data-focus="remove-${file.key}" ${busy || file.status === "loading" ? "disabled" : ""}>×</button></div>`).join("")}</div>
      <div class="formats"><span class="eyebrow">SUPPORTED FORMATS</span><p>.mxml · .musicxml · .xml · .mxl</p><p>Each score is arranged separately.</p></div></aside>
      <section class="editor" aria-label="Score arrangement">
      ${
        !item
          ? `<div class="empty"><svg class="score-mark" viewBox="0 0 120 120" aria-hidden="true" focusable="false"><path d="M10 35h100M10 49h100M10 63h100M10 77h100M10 91h100" stroke="currentColor" stroke-width="2" opacity=".4"/><path d="M47 85V27l42-9v58" fill="none" stroke="currentColor" stroke-width="6" stroke-linejoin="round"/><ellipse cx="38" cy="86" rx="13" ry="9" transform="rotate(-20 38 86)" fill="currentColor"/><ellipse cx="80" cy="77" rx="13" ry="9" transform="rotate(-20 80 77)" fill="currentColor"/></svg><span class="eyebrow">MAKE SPACE FOR YOUR MUSIC</span><h2>Start with a score.</h2><p>Drop your MusicXML files here to see their instruments, then merge parts or split them into new staves.</p><button type="button" data-action="open" data-focus="empty-open">Choose files</button></div>`
          : !ready
            ? `<div class="empty"><span class="eyebrow">${item.status === "loading" ? "READING YOUR SCORE" : "THIS SCORE NEEDS ATTENTION"}</span><h2>${escape(item.name)}</h2><p class="${item.status === "error" ? "error-text" : ""}">${escape(item.error ?? "Parsing instruments and checking the score…")}</p></div>`
            : `
        <div class="score-heading"><div><span class="eyebrow">ARRANGE YOUR SCORE</span><h2>${escape(score.name)}</h2><p>${score.parts.length} instruments · Source files stay intact</p></div><span class="score-badge">MusicXML</span></div>
        <fieldset class="operation"><legend class="sr-only">Choose an operation</legend><label class="operation-choice ${state.kind === "split" ? "chosen" : ""}"><input type="radio" name="operation" value="split" data-focus="operation-split" ${state.kind === "split" ? "checked" : ""} ${busy ? "disabled" : ""}><span><strong>Split instruments</strong><small>Separate chord tones or voices</small></span></label><label class="operation-choice ${state.kind === "merge" ? "chosen" : ""}"><input type="radio" name="operation" value="merge" data-focus="operation-merge" ${state.kind === "merge" ? "checked" : ""} ${busy ? "disabled" : ""}><span><strong>Merge instruments</strong><small>Bring parts into a single staff</small></span></label></fieldset>
        <div class="instrument-heading"><div><h3>${state.kind === "split" ? "Choose instruments to split" : "Choose instruments for a group"}</h3><p>${state.kind === "split" ? "Originals are kept. New staves appear immediately below each original." : "Each instrument can belong to one group. Other parts stay as they are."}</p></div><button type="button" class="text-button" data-action="select-all" data-focus="select-all" ${busy ? "disabled" : ""}>${score.parts.filter((p) => state.kind !== "merge" || !assigned.has(p.id)).every((p) => state.selected.has(p.id)) ? "Clear selection" : "Select all"}</button></div>
        <div class="instruments" role="group" aria-label="Instruments">${score.parts.map((part, index) => `<div class="instrument-entry"><label class="instrument ${state.selected.has(part.id) ? "checked" : ""} ${state.kind === "merge" && assigned.has(part.id) ? "assigned" : ""}"><input type="checkbox" data-part="${escape(part.id)}" data-focus="part-${index}" ${state.selected.has(part.id) ? "checked" : ""} ${busy || (state.kind === "merge" && assigned.has(part.id)) ? "disabled" : ""}><span class="part-number">${String(index + 1).padStart(2, "0")}</span><span class="part-name"><strong>${escape(part.name || part.id)}</strong><small>${escape(part.instrument_names.join(", ") || "Unnamed instrument")} · ${escape(part.id)}</small></span><span class="voice-count">${state.kind === "merge" && assigned.has(part.id) ? "Grouped" : `${part.voice_count} ${part.voice_count === 1 ? "voice" : "voices"}`}</span></label>${splitPreview(part, index, state)}</div>`).join("")}</div>
        ${state.kind === "split" ? `<div class="split-options"><label for="split-mode">Split by</label><select id="split-mode" data-focus="split-mode" ${busy ? "disabled" : ""}><option value="chords" ${state.mode === "chords" ? "selected" : ""}>Chord tones</option><option value="voices" ${state.mode === "voices" ? "selected" : ""}>Explicit voices</option></select><p>${state.mode === "chords" ? "Separate simultaneous notes by pitch into individual staves." : "Separate the voices already written into the score."}</p></div>` : `<div class="merge-options"><button type="button" data-action="add-group" data-focus="add-group" ${busy || state.selected.size < 2 ? "disabled" : ""}>＋ Create merge group</button><span>${state.selected.size} selected</span></div><div class="groups">${state.groups.map((group, gi) => `<section class="merge-group" aria-label="Merge group ${gi + 1}"><div class="group-heading"><h4>Group ${gi + 1}</h4><button type="button" class="text-button" data-action="remove-group" data-group="${gi}" data-focus="remove-group-${gi}" ${busy ? "disabled" : ""}>Remove group</button></div><ol>${group.map((id, pi) => `<li><span>${escape(score.parts.find((p) => p.id === id)?.name || id)} <small>${escape(id)}</small></span><span class="reorder"><button type="button" aria-label="Move ${escape(id)} up in group ${gi + 1}" data-action="up" data-group="${gi}" data-index="${pi}" data-focus="up-${gi}-${pi}" ${busy || pi === 0 ? "disabled" : ""}>↑</button><button type="button" aria-label="Move ${escape(id)} down in group ${gi + 1}" data-action="down" data-group="${gi}" data-index="${pi}" data-focus="down-${gi}-${pi}" ${busy || pi === group.length - 1 ? "disabled" : ""}>↓</button></span></li>`).join("")}</ol><p class="hint">Top to bottom sets voice order in the merged staff.</p></section>`).join("")}</div>`}
        <footer class="export-bar"><p>Export a new score.<br><span>${loading ? "Wait for queued scores to finish loading." : "Open the exported file to arrange it further."}</span></p><button type="button" class="primary" data-action="export" data-focus="export" ${busy || loading ? "disabled" : ""}>${busy ? "Exporting…" : "Export score"}<span aria-hidden="true"> →</span></button></footer>
      `
      }
      </section>
    </main>
    <div class="status ${messageError ? "error-text" : ""}" role="${messageError ? "alert" : "status"}" aria-live="polite">${escape(message || "Your music stays on this device.")}</div>
    ${dropping ? '<div class="drop-overlay" aria-hidden="true"><div>Drop scores to open them<span>MusicXML · Compressed MusicXML</span></div></div>' : ""}`;
  if (ready && state.kind === "split")
    score.parts.forEach((part, index) =>
      part.split_plans[state.mode].forEach((lane, li) => {
        const select = root.querySelector<HTMLSelectElement>(`#fallback-${index}-${li}`);
        const fallback = fallbackVoice(state, part.id, lane);
        if (select) select.value = fallback === null ? "none" : `voice:${fallback}`;
      }),
    );
  if (focus)
    root
      .querySelector<HTMLElement>(`[data-focus="${CSS.escape(focus)}"]`)
      ?.focus({ preventScroll: true });
  root.querySelector(".editor")!.scrollTop = editorTop;
  root.querySelector(".file-list")!.scrollTop = filesTop;
  window.scrollTo(pageLeft, pageTop);
  renderedKey = selectedKey;
}

async function addFiles(paths: string[]): Promise<void> {
  if (busy)
    throw new Error("Finish the current export before opening more files.");
  const added = paths.filter(
    (path, index) =>
      paths.indexOf(path) === index &&
      !queue.some((item) => item.path === path),
  );
  await Promise.all(
    added.map(async (path) => {
      const item: QueueItem = {
        key: nextKey++,
        path,
        name: path.split(/[\\/]/).pop() || path,
        status: "loading",
        arrangement: newArrangement(),
      };
      queue.push(item);
      selectedKey ??= item.key;
      render();
      try {
        item.document = await invoke<Inspection>("inspect_file", { path });
        item.name = item.document.name;
        item.status = item.document.error ? "error" : "ready";
        item.error = item.document.error ?? undefined;
      } catch (error) {
        item.status = "error";
        item.error = String(error);
      }
      render();
    }),
  );
}

root.addEventListener("click", async (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>(
    "button[data-action]",
  );
  if (!button || button.disabled) return;
  const action = button.dataset.action;
  const item = selected();
  try {
    if (action === "open") {
      await addFiles(await invoke<string[]>("open_files"));
    } else if (action === "select") {
      selectedKey = Number(button.dataset.key);
      message = "";
      render();
    } else if (action === "remove") {
      const index = queue.findIndex(
        (entry) => entry.key === Number(button.dataset.key),
      );
      if (index < 0) return;
      const removed = queue[index];
      if (removed.document)
        await invoke("release_document", { id: removed.document.id });
      queue.splice(index, 1);
      if (selectedKey === removed.key)
        selectedKey = queue[Math.min(index, queue.length - 1)]?.key ?? null;
      render();
    } else if (item?.document && action === "select-all") {
      const assigned = new Set(item.arrangement.groups.flat());
      const available = item.document.parts.filter(
        (part) => item.arrangement.kind !== "merge" || !assigned.has(part.id),
      );
      item.arrangement.selected = available.every((part) =>
        item.arrangement.selected.has(part.id),
      )
        ? new Set()
        : new Set(available.map((part) => part.id));
      render();
    } else if (item?.document && action === "add-group") {
      const error = addGroup(item.arrangement, item.document.parts);
      notify(
        error ?? "Merge group created. Reorder its instruments if needed.",
        !!error,
      );
    } else if (item && action === "remove-group") {
      item.arrangement.groups.splice(Number(button.dataset.group), 1);
      render();
    } else if (item && (action === "up" || action === "down")) {
      movePart(
        item.arrangement.groups[Number(button.dataset.group)],
        Number(button.dataset.index),
        action === "up" ? -1 : 1,
      );
      render();
    } else if (item?.document && action === "export") {
      if (queue.some((entry) => entry.status === "loading")) {
        throw new Error(
          "Wait for queued scores to finish loading before exporting.",
        );
      }
      const request = operation(item.arrangement, item.document.parts);
      busy = true;
      notify("Choose where to save your arranged score.");
      try {
        const result = await invoke<{ path: string; warnings: string[] } | null>("export_document", {
          id: item.document.id,
          operation: request,
        });
        notify(result ? [
          `Exported ${result.path}`,
          ...(result.warnings.length ? ["Review warnings — affected passages are marked red:", ...result.warnings] : []),
        ].join("\n") : "Export cancelled.");
      } finally {
        busy = false;
        render();
      }
    }
  } catch (error) {
    busy = false;
    notify(String(error), true);
  }
});

root.addEventListener("change", (event) => {
  const input = event.target as HTMLInputElement | HTMLSelectElement;
  const item = selected();
  if (!item || busy) return;
  if (input instanceof HTMLInputElement && input.name === "operation") {
    item.arrangement.kind = input.value as "split" | "merge";
    item.arrangement.selected.clear();
    message = "";
  } else if (input instanceof HTMLInputElement && input.dataset.part) {
    if (input.checked) item.arrangement.selected.add(input.dataset.part);
    else item.arrangement.selected.delete(input.dataset.part);
  } else if (input.id === "split-mode") {
    item.arrangement.mode = input.value as "chords" | "voices";
  } else if (input.dataset.fallbackPart && item.document) {
    const part = item.document.parts.find((part) => part.id === input.dataset.fallbackPart);
    const lane = part?.split_plans[item.arrangement.mode][Number(input.dataset.lane)];
    if (part && lane)
      setFallbackVoice(item.arrangement, part.id, lane, input.value === "none" ? null : input.value.slice(6));
  }
  render();
});

// Prevent browser navigation; native Tauri events supply real filesystem paths.
window.addEventListener("dragover", (event) => event.preventDefault());
window.addEventListener("drop", (event) => event.preventDefault());
render();
getCurrentWebviewWindow()
  .onDragDropEvent((event) => {
    dropping = event.payload.type === "enter" || event.payload.type === "over";
    render();
    if (event.payload.type === "drop") {
      if (busy)
        notify("Finish the current export before opening more files.", true);
      else
        void addFiles(event.payload.paths).catch((error) =>
          notify(String(error), true),
        );
    }
  })
  .catch((error) =>
    notify(
      `Native file dropping is unavailable: ${String(error)}. Use Open files.`,
      true,
    ),
  );
