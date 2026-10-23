import type { PracticeConfig, PracticeEnvironment, PracticeOptions, PracticePart, PracticeProgress, PracticeResult, PresetId } from "./model.ts";

export function presetValue(sound: PresetId | null): string {
  return sound ? `${sound.bank}:${sound.program}` : "";
}

export function renderPractice(
  parts: PracticePart[], options: PracticeOptions, config: PracticeConfig,
  environment: PracticeEnvironment | null, checking: boolean, busy: boolean,
  progress: PracticeProgress | undefined, result: PracticeResult | undefined,
  escape: (value: string) => string,
): string {
  const disabled = busy || checking ? "disabled" : "";
  const soundOptions = (sound: PresetId | null) => {
    const presets = environment?.presets ?? [];
    const current = presetValue(sound);
    const missing = sound && !presets.some((preset) => presetValue(preset) === current);
    return `<option value="">Choose a sound…</option>${missing ? `<option value="${current}" selected>Unavailable preset ${current}</option>` : ""}${presets.map((preset) => `<option value="${presetValue(preset)}" ${current === presetValue(preset) ? "selected" : ""}>${escape(preset.name)} · ${preset.bank}:${preset.program}</option>`).join("")}`;
  };
  return `<section class="practice-environment" aria-label="MuseScore renderer">
    <h3>MuseScore renderer</h3><p class="hint">MuseScore 4.7.x and its matching MS Basic SoundFont are required for MP3 export. You can inspect scores before configuring them.</p>
    ${(["executable", "soundfont"] as const).map((kind) => `<div class="practice-path"><label for="practice-${kind}">${kind === "executable" ? "MuseScore executable" : "MS Basic SoundFont"}</label><input id="practice-${kind}" data-practice-path="${kind}" data-focus="practice-${kind}" value="${escape(config[kind] ?? "")}" placeholder="${escape(environment?.[kind] ?? "Detect automatically")}" ${disabled}><button type="button" data-action="practice-browse" data-kind="${kind}" data-focus="browse-${kind}" ${disabled}>Browse<span class="sr-only"> ${kind === "executable" ? "MuseScore executable" : "MS Basic SoundFont"}</span></button></div>`).join("")}
    <button type="button" class="text-button" data-action="practice-detect" data-focus="practice-detect" ${disabled}>${checking ? "Checking renderer…" : "Check renderer and sounds"}</button>
    ${environment?.version ? `<p>Detected MuseScore ${escape(environment.version)}</p>` : ""}
    ${environment?.error ? `<p class="error-text" role="alert">${escape(environment.error)}</p>` : ""}
    <p class="hint">Keep MuseScore closed during rendering. VoiceWeaver temporarily controls the built-in metronome preference and restores it when rendering ends.</p>
  </section>
  <div class="instrument-heading"><div><h3>Practice instruments</h3><p>The featured instrument uses its foreground sound. Other selected background instruments play more quietly.</p></div></div>
  <div class="practice-parts" role="group" aria-label="Practice instruments">${parts.map((part, index) => {
    const setting = options.parts[index];
    const label = `${part.name || "Unnamed instrument"} · Instrument ${index + 1}`;
    return `<fieldset class="practice-part"><legend>${escape(label)}${part.visible ? "" : " · Hidden in score"}</legend><p class="hint">${escape(part.original_instruments.join(", ") || "Unnamed instrument")}</p>
      <div class="practice-membership"><label><input type="checkbox" data-practice-index="${index}" data-practice-field="include_in_background" data-focus="practice-background-${index}" ${setting.include_in_background ? "checked" : ""} ${busy ? "disabled" : ""}> Include in background<span class="sr-only"> · ${escape(label)}</span></label><label><input type="checkbox" data-practice-index="${index}" data-practice-field="create_track" data-focus="practice-target-${index}" ${setting.create_track ? "checked" : ""} ${busy ? "disabled" : ""}> Create practice track<span class="sr-only"> · ${escape(label)}</span></label></div>
      ${(["foreground_sound", "background_sound"] as const).map((field) => `<label class="practice-sound" for="${field}-${index}">${field === "foreground_sound" ? "Foreground sound" : "Background sound"}<span class="sr-only"> · ${escape(label)}</span><select id="${field}-${index}" data-practice-index="${index}" data-practice-field="${field}" data-focus="${field}-${index}" ${disabled}>${soundOptions(setting[field])}</select></label>`).join("")}
    </fieldset>`;
  }).join("")}</div>
  <fieldset class="practice-mix"><legend>Practice mix</legend><label for="background-reduction">Background reduction (dB)</label><input type="number" id="background-reduction" data-focus="background-reduction" min="0" max="60" step="1" value="${options.background_reduction_db}" ${busy ? "disabled" : ""}><p class="hint">Foreground stays at 0 dB. Background instruments are reduced by this amount.</p><label><input type="checkbox" id="practice-metronome" data-focus="practice-metronome" ${options.metronome_enabled ? "checked" : ""} ${busy ? "disabled" : ""}> Enable metronome</label><label for="metronome-level">Metronome level (dB)</label><input type="number" id="metronome-level" data-focus="metronome-level" min="-60" max="0" step="1" value="${options.metronome_db}" ${busy || !options.metronome_enabled ? "disabled" : ""}><p class="hint">Uses MuseScore’s built-in metronome. No count-in is added.</p></fieldset>
  ${progress ? `<p class="practice-progress" role="status">${escape(progress.stage)} · ${progress.completed} / ${progress.total} tracks completed</p>` : ""}
  ${result ? `<section class="practice-results" aria-label="Practice track results"><h3>${result.cancelled ? "Rendering cancelled" : result.error ? "Rendering stopped" : "Practice tracks created"}</h3>${result.error ? `<p class="error-text">${escape(result.error)}</p>` : ""}<p>${result.completed.length} completed ${result.completed.length === 1 ? "track" : "tracks"} saved in ${escape(result.directory)}</p><ul>${result.completed.map((track) => `<li>${escape(track.name)}<small>${escape(track.path)}</small></li>`).join("")}</ul></section>` : ""}
  <footer class="export-bar"><p>Create MP3 practice tracks.<br><span>192 kbps · Source score stays intact</span></p>${busy ? `<button type="button" data-action="practice-cancel" data-focus="practice-cancel">Cancel rendering</button>` : ""}<button type="button" class="primary" data-action="practice-export" data-focus="practice-export" ${busy || checking ? "disabled" : ""}>${busy ? "Rendering…" : "Create Practice Tracks"}</button></footer>`;
}
