import { Button } from "./components/ui/button.tsx";
import { Checkbox } from "./components/ui/checkbox.tsx";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./components/ui/select.tsx";
import { isTransformable, movePart, stemDirection } from "./model.ts";
import type { SplitWorkspaceProps } from "./split-workspace.tsx";

export function MergeWorkspace({ parts, state, busy, exporting, onChange, onExport, onCreateGroup }: SplitWorkspaceProps & { onCreateGroup: () => void }) {
  const assigned = new Set(state.groups.flat());
  const available = parts.filter((part) => isTransformable(part) && !assigned.has(part.id));
  const selected = available.filter((part) => state.selected.has(part.id));
  const allSelected = available.length > 0 && selected.length === available.length;
  const name = (id: string) => parts.find((part) => part.id === id)?.name || id;
  return <div className="arrangement-workspace">
    <div className="workspace-layout merge-workspace">
      <section className="pane source-pane" aria-label="Merge sources">
        <div className="pane-header"><h2>Instruments</h2>
          <Button variant="ghost" size="sm" data-action="select-all" disabled={busy || !available.length}
            onClick={() => onChange((next) => { next.selected = allSelected ? new Set() : new Set(available.map((part) => part.id)); })}>
            {allSelected ? "Clear selection" : "Select all"}
          </Button>
        </div>
        <div className="pane-body instruments" role="group" aria-label="Instruments">
          {parts.map((part) => <div className="source-row" key={part.id}>
            <Checkbox id={`merge-source-${part.id}`} data-part={part.id} checked={isTransformable(part) && !assigned.has(part.id) && state.selected.has(part.id)} disabled={busy || assigned.has(part.id) || !isTransformable(part)}
              aria-describedby={!isTransformable(part) ? `merge-reason-${part.id}` : undefined}
              onCheckedChange={(checked) => onChange((next) => { if (checked) next.selected.add(part.id); else next.selected.delete(part.id); })} />
            <label className="source-copy" htmlFor={`merge-source-${part.id}`}><span className="part-name">{part.name || part.id}</span>
              {part.visible === false && <span className="muted source-visibility">Hidden in source</span>}
              {isTransformable(part) ? <span className="muted source-count">{assigned.has(part.id) ? "Grouped" : `${part.voice_count} ${part.voice_count === 1 ? "voice" : "voices"}`}</span>
                : <span className="muted source-count" id={`merge-reason-${part.id}`}>{(part.staff_count ?? 1) > 1 ? `${part.staff_count} source staves · Multi-staff transformation is unsupported.` : "Transformation is unsupported."} Retained in export.</span>}
            </label>
          </div>)}
          <div className="merge-create"><p className="muted">{selected.length ? `${selected.length} instruments selected` : "No instruments selected"}</p>
            <Button data-action="add-group" variant="outline" disabled={busy || selected.length < 2} onClick={onCreateGroup}>Create merge group</Button>
          </div>
          <p className="pane-note muted">All original instruments stay in the export with their source visibility. Merged instruments are visible, including results from hidden sources.</p>
        </div>
      </section>
      <section className="pane configuration-pane" aria-label="Merge configuration">
        <div className="pane-header"><h2>Merge configuration</h2><p className="muted">Set the stem direction for each source staff.</p></div>
        <div className="pane-body configuration-body">
          {state.groups.length ? <table className="configuration-table"><thead><tr><th scope="col">Source staff</th><th scope="col">Stem direction</th></tr></thead>
            {state.groups.map((group, groupIndex) => <tbody className="merge-group" aria-label={`Merge group ${groupIndex + 1}`} key={groupIndex}>
              <tr className="configuration-group-header"><th scope="rowgroup" colSpan={2}><div className="merge-group-header">
                <span>Group {groupIndex + 1}<span className="muted group-summary">{group.length} source staves → 1 staff</span></span>
                <Button size="sm" variant="ghost" data-action="remove-group" data-group={groupIndex} disabled={busy}
                  aria-label={`Remove group ${groupIndex + 1}`} onClick={() => onChange((next) => { next.groups.splice(groupIndex, 1); })}>Remove group</Button>
              </div></th></tr>
              {group.map((id, index) => <tr className="merge-part" key={id}>
                <th scope="row" className="group-part"><span>{name(id)}</span></th>
                <td><Select value={stemDirection(state, id, index)} disabled={busy}
                  onValueChange={(value) => { if (value === "up" || value === "down") onChange((next) => { next.stems.set(id, value); }); }}>
                  <SelectTrigger id={`stem-${id}`} className="score-select" aria-label={`Stem direction for ${name(id)} ${id}, group ${groupIndex + 1}`}><SelectValue>{stemDirection(state, id, index) === "up" ? "Up" : "Down"}</SelectValue></SelectTrigger>
                  <SelectContent><SelectItem value="up">Up</SelectItem><SelectItem value="down">Down</SelectItem></SelectContent>
                </Select></td>
              </tr>)}
            </tbody>)}
          </table> : <div className="merge-empty workspace-empty"><strong>No merge groups yet</strong><p className="muted">Select two or more instruments, then choose “Create merge group”.</p></div>}
          <p className="pane-note muted">Each source staff has its own stem direction. Up stems share voice 1; down stems share voice 2. Different rhythms use extra voices.</p>
        </div>
      </section>
      <section className="pane options-pane" aria-label="Source order">
        <div className="pane-header"><h2>Source order</h2></div>
        <div className="pane-body options-body">
          {state.groups.map((group, groupIndex) => <section className="merge-order-group" aria-label={`Source order for group ${groupIndex + 1}`} key={groupIndex}>
            <h3>Group {groupIndex + 1}</h3><p className="muted">Reorder sources within this group.</p>
            <ol className="merge-source-order">{group.map((id, index) => <li className="merge-order-row" key={id}>
              <span>{index + 1}. {name(id)}</span><span className="reorder">
                <Button size="icon" variant="ghost" aria-label={`Move ${name(id)}, source ${index + 1}, up in group ${groupIndex + 1}`}
                  data-action="up" data-group={groupIndex} data-index={index} disabled={busy || index === 0}
                  onClick={() => onChange((next) => movePart(next.groups[groupIndex], index, -1))}>↑</Button>
                <Button size="icon" variant="ghost" aria-label={`Move ${name(id)}, source ${index + 1}, down in group ${groupIndex + 1}`}
                  data-action="down" data-group={groupIndex} data-index={index} disabled={busy || index === group.length - 1}
                  onClick={() => onChange((next) => movePart(next.groups[groupIndex], index, 1))}>↓</Button>
              </span>
            </li>)}</ol>
          </section>)}
          <p className="pane-note muted">Order applies to the whole merge group.</p>
        </div>
      </section>
    </div>
    <footer className="workspace-footer"><span className="muted">{state.groups.length} merge {state.groups.length === 1 ? "group" : "groups"} · {assigned.size} source instruments</span>
      <Button data-action="export" disabled={busy || !state.groups.length} onClick={onExport}>{exporting ? "Exporting…" : "Export score"}</Button>
    </footer>
  </div>;
}
