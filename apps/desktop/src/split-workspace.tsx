import { Button } from "./components/ui/button.tsx";
import { Checkbox } from "./components/ui/checkbox.tsx";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./components/ui/select.tsx";
import { fallbackVoice, isTransformable, setFallbackVoice, type Arrangement, type Part } from "./model.ts";

export interface SplitWorkspaceProps {
  parts: Part[];
  state: Arrangement;
  busy: boolean;
  exporting: boolean;
  onChange: (change: (state: Arrangement) => void) => void;
  onExport: () => void;
}

export function SplitWorkspace({ parts, state, busy, exporting, onChange, onExport }: SplitWorkspaceProps) {
  const available = parts.filter(isTransformable);
  const selected = available.filter((part) => state.selected.has(part.id));
  const generated = selected.reduce((total, part) => total + part.split_plans[state.mode].length, 0);
  const allSelected = available.length > 0 && selected.length === available.length;
  return <div className="arrangement-workspace">
    <div className="workspace-layout split-workspace">
      <section className="pane source-pane" aria-label="Source staves">
        <div className="pane-header"><h2>Source staves</h2>
          <Button variant="ghost" size="sm" data-action="select-all" disabled={busy || !available.length}
            onClick={() => onChange((next) => { next.selected = allSelected ? new Set() : new Set(available.map((part) => part.id)); })}>
            {allSelected ? "Clear selection" : "Select all"}
          </Button>
        </div>
        <div className="pane-body instruments" role="group" aria-label="Instruments">
          {[true, false].map((visible) => {
            const group = parts.filter((part) => (part.visible !== false) === visible);
            if (!group.length) return null;
            const heading = visible ? "Visible instruments" : "Hidden instruments";
            return <section className="instrument-visibility-group" aria-label={heading} key={heading}>
              <h3 className="instrument-visibility-heading">{heading}</h3>
              {group.map((part) => {
                const lanes = part.split_plans[state.mode].length;
                const supported = isTransformable(part);
                return <div className="source-row" key={part.id}>
                  <Checkbox id={`split-source-${part.id}`} data-part={part.id} checked={supported && state.selected.has(part.id)} disabled={busy || !supported}
                    aria-describedby={!supported ? `split-reason-${part.id}` : undefined}
                    onCheckedChange={(checked) => onChange((next) => { if (checked) next.selected.add(part.id); else next.selected.delete(part.id); })} />
                  <label className="source-copy" htmlFor={`split-source-${part.id}`}><span className="part-name">{part.name || part.id}</span>
                    {supported ? <span className="muted source-count">{lanes} new {lanes === 1 ? "staff" : "staves"}</span>
                      : <span className="muted source-count" id={`split-reason-${part.id}`}>{(part.staff_count ?? 1) > 1 ? `${part.staff_count} source staves · Multi-staff transformation is unsupported.` : "Transformation is unsupported."} Retained in export.</span>}
                  </label>
                </div>;
              })}
            </section>;
          })}
          <p className="pane-note muted">All original instruments stay in the export with their source visibility. Generated staves are visible, including those from hidden sources.</p>
        </div>
      </section>
      <section className="pane configuration-pane" aria-label="Fallback configuration">
        <div className="pane-header"><h2>Fallback configuration</h2><p className="muted">Configure each generated staff.</p></div>
        <div className="pane-body configuration-body">
          {selected.length ? <table className="configuration-table"><thead><tr>
            <th scope="col">New staff <span className="staff-source-compact">/ source</span></th><th scope="col" className="source-column">Source</th><th scope="col">Fallback voice</th>
          </tr></thead>{selected.map((part) => {
            const lanes = part.split_plans[state.mode];
            const voices = [...new Set([...part.split_plans.voices, ...lanes].map((lane) => lane.voice))];
            return <tbody className="split-preview" aria-label={`Split preview for ${part.name || part.id} ${part.id}`} key={part.id}>
              <tr className="configuration-group-header"><th scope="rowgroup" colSpan={3}>{part.name || part.id}
                <span className="sr-only"> · {lanes.length} additional {lanes.length === 1 ? "staff" : "staves"}</span>
              </th></tr>
              {lanes.map((lane, index) => {
                const source = `Voice ${lane.voice}${state.mode === "chords" ? ` · Chord tone ${lane.tone + 1}` : ""}`;
                const fallback = fallbackVoice(state, part.id, lane);
                return <tr className="fallback-row" key={`${lane.voice}:${lane.tone}`}>
                  <th scope="row"><span id={`staff-${part.id}-${lane.voice}-${lane.tone}`}>Additional staff {index + 1}</span>
                    <span className="muted staff-source-compact">{source}</span>
                  </th>
                  <td className="muted source-column">{source}</td>
                  <td><Select value={fallback === null ? "none" : `voice:${fallback}`} disabled={busy}
                    onValueChange={(value) => { if (value !== null) onChange((next) => setFallbackVoice(next, part.id, lane, value === "none" ? null : value.slice(6))); }}>
                    <SelectTrigger id={`fallback-${part.id}-${lane.voice}-${lane.tone}`} className="score-select"
                      aria-label={`Fallback voice for ${part.name || part.id} ${part.id}, additional staff ${index + 1}, ${source}`}>
                      <SelectValue>{fallback === null ? "None (silent)" : `Voice ${fallback}`}</SelectValue>
                    </SelectTrigger>
                    <SelectContent><SelectItem value="none">None (silent)</SelectItem>
                      {voices.filter((voice) => voice !== lane.voice).map((voice) => <SelectItem value={`voice:${voice}`} key={voice}>Voice {voice}</SelectItem>)}
                    </SelectContent>
                  </Select></td>
                </tr>;
              })}
            </tbody>;
          })}</table> : <p className="workspace-empty muted">Select source staves to configure their generated staves.</p>}
          <p className="pane-note muted">Each generated staff has its own fallback voice. Originals stay in place. Fallbacks are used only when a staff’s own voice is absent for a whole measure.</p>
        </div>
      </section>
      <section className="pane options-pane options-card" aria-label="Global split options">
        <div className="pane-header"><h2>Global options</h2></div>
        <div className="pane-body options-body">
          <div className="field"><label id="split-mode-label" className="muted" htmlFor="split-mode">Split by</label>
            <Select value={state.mode} disabled={busy} onValueChange={(value) => { if (value === "chords" || value === "voices") onChange((next) => { next.mode = value; }); }}>
              <SelectTrigger id="split-mode" aria-labelledby="split-mode-label"><SelectValue>{state.mode === "chords" ? "Chord tones" : "Explicit voices"}</SelectValue></SelectTrigger>
              <SelectContent><SelectItem value="chords">Chord tones</SelectItem><SelectItem value="voices">Explicit voices</SelectItem></SelectContent>
            </Select>
            <p className="muted">{state.mode === "chords" ? "Separate simultaneous notes by pitch." : "Separate the voices already written into the score."}</p>
          </div>
          <div className="option-section"><div className="checkbox-field">
            <Checkbox id="insert-fallback-slurs" checked={state.insertFallbackSlurs} disabled={busy}
              onCheckedChange={(checked) => onChange((next) => { next.insertFallbackSlurs = checked; })} />
            <label htmlFor="insert-fallback-slurs">Automatically insert slurs at fallback voice transitions</label>
          </div><p className="muted">Matching pitches continue existing ties. When pitches differ, a slur marks endpoint notes red for review. When disabled, the fallback note becomes a rest of the same duration.</p></div>
          <p className="pane-note muted">Applies to all selected staves.</p>
        </div>
      </section>
    </div>
    <footer className="workspace-footer"><span className="muted">{selected.length} source {selected.length === 1 ? "staff" : "staves"} selected · {generated} new {generated === 1 ? "staff" : "staves"}</span>
      <Button data-action="export" disabled={busy || !selected.length} onClick={onExport}>{exporting ? "Exporting…" : "Export score"}</Button>
    </footer>
  </div>;
}
