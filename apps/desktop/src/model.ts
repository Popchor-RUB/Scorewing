export interface Part {
  id: string;
  name: string;
  instrument_names: string[];
  voice_count: number;
}

export interface Inspection {
  id: string;
  path: string;
  name: string;
  parts: Part[];
  error: string | null;
}

export type Operation =
  | { kind: "split"; part_ids: string[]; mode: "chords" | "voices" }
  | { kind: "merge"; groups: string[][] };

export interface Arrangement {
  kind: "split" | "merge";
  mode: "chords" | "voices";
  selected: Set<string>;
  groups: string[][];
}

export function newArrangement(): Arrangement {
  return { kind: "split", mode: "chords", selected: new Set(), groups: [] };
}

export function addGroup(
  arrangement: Arrangement,
  parts: Part[],
): string | null {
  const assigned = new Set(arrangement.groups.flat());
  const ids = parts
    .filter((p) => arrangement.selected.has(p.id))
    .map((p) => p.id);
  if (ids.length < 2)
    return "Select at least two instruments to create a merge group.";
  if (ids.some((id) => assigned.has(id)))
    return "An instrument can belong to only one merge group.";
  arrangement.groups.push(ids);
  arrangement.selected.clear();
  return null;
}

export function movePart(group: string[], index: number, offset: -1 | 1): void {
  const next = index + offset;
  if (index < 0 || index >= group.length || next < 0 || next >= group.length)
    return;
  [group[index], group[next]] = [group[next], group[index]];
}

export function operation(arrangement: Arrangement, parts: Part[]): Operation {
  const known = new Set(parts.map((part) => part.id));
  if (arrangement.kind === "split") {
    const part_ids = parts
      .filter((p) => arrangement.selected.has(p.id))
      .map((p) => p.id);
    if (!part_ids.length)
      throw new Error("Select at least one instrument to split.");
    return { kind: "split", part_ids, mode: arrangement.mode };
  }
  if (!arrangement.groups.length)
    throw new Error("Create at least one merge group.");
  const used = new Set<string>();
  for (const group of arrangement.groups) {
    if (group.length < 2)
      throw new Error("Each merge group needs at least two instruments.");
    for (const id of group) {
      if (!known.has(id) || used.has(id))
        throw new Error(
          "Merge groups must contain distinct instruments from this score.",
        );
      used.add(id);
    }
  }
  return {
    kind: "merge",
    groups: arrangement.groups.map((group) => [...group]),
  };
}
