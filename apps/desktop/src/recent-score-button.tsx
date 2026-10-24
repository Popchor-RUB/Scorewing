import { useEffect, useState } from "react";
import { Button, ButtonGroup, Description, Dropdown, Label } from "@heroui/react";
import { invoke } from "@tauri-apps/api/core";

export function RecentScoreButton({ paths, disabled, reading, onOpen }: {
  paths: string[]; disabled: boolean; reading: boolean; onOpen: (paths?: string[]) => void;
}) {
  const [isOpen, setOpen] = useState(false);
  const [existing, setExisting] = useState<string[] | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (disabled) { setOpen(false); return; }
    if (!isOpen) return;
    let active = true;
    void invoke<string[]>("existing_score_paths", { paths }).then(
      (result) => { if (active) setExisting(result); },
      () => { if (active) setFailed(true); },
    );
    return () => { active = false; };
  }, [isOpen, disabled, paths]);

  function changeOpen(open: boolean) {
    setExisting(null);
    setFailed(false);
    setOpen(open);
  }

  const status = failed ? "Recent scores unavailable" : existing === null ? "Checking recent scores…" : "No recent scores";
  return <ButtonGroup aria-label="Open a score" isDisabled={disabled}>
    <Button data-action="open" onPress={() => onOpen()}>{reading ? "Reading your score…" : "Choose a score"}</Button>
    <Dropdown isOpen={isOpen && !disabled} onOpenChange={changeOpen}>
      <Button isIconOnly isDisabled={disabled} aria-label="Recently opened scores"><span aria-hidden="true">⌄</span></Button>
      <Dropdown.Popover placement="bottom end" className="max-w-[calc(100vw-2rem)]">
        <Dropdown.Menu aria-label="Recently opened scores" onAction={(path) => { changeOpen(false); onOpen([String(path)]); }}>
          {existing?.length ? existing.map((path) => <Dropdown.Item key={path} id={path} textValue={path}>
            <div className="min-w-0 max-w-sm text-left"><Label className="block wrap-anywhere">{path.split(/[\\/]/).pop()}</Label><Description className="wrap-anywhere">{path}</Description></div>
          </Dropdown.Item>) : <Dropdown.Item id="recent-status" isDisabled textValue={status}>{status}</Dropdown.Item>}
        </Dropdown.Menu>
      </Dropdown.Popover>
    </Dropdown>
  </ButtonGroup>;
}
