import { useEffect, useRef } from "react";
import { Button, Description, Label, Modal } from "@heroui/react";
import type { Inspection } from "./model.ts";

export function DiffFileSelectors({ older, newer, disabled, onChoose, onSwap }: {
  older?: Inspection; newer: Inspection; disabled: boolean;
  onChoose: (version: "old" | "new") => void; onSwap: () => void;
}) {
  return <div className="flex flex-col gap-4">
    <div data-version="old" className="min-w-0 space-y-2">
      <Label>Older version</Label>
      <Button variant="secondary" className="h-auto w-full justify-start whitespace-normal py-3 text-left wrap-anywhere" data-action="open-older"
        aria-label={older ? "Replace older version" : "Choose older version"} isDisabled={disabled} onPress={() => onChoose("old")}>
        {older?.name ?? "Choose older version"}
      </Button>
      {older && <Description className="wrap-anywhere">{older.path}</Description>}
    </div>
    <Button isIconOnly variant="secondary" className="self-center" aria-label="Swap old and new files" data-action="swap-diff"
      isDisabled={disabled || !older} onPress={onSwap}><span aria-hidden="true">⇅</span></Button>
    <div data-version="new" className="min-w-0 space-y-2">
      <Label>Newer version</Label>
      <Button variant="secondary" className="h-auto w-full justify-start whitespace-normal py-3 text-left wrap-anywhere" data-action="open-newer"
        aria-label="Replace newer version" isDisabled={disabled} onPress={() => onChoose("new")}>{newer.name}</Button>
      <Description className="wrap-anywhere">{newer.path}</Description>
    </div>
  </div>;
}

export function DiffPairModal({ pair, busy, closing, error, status, onChoose, onSwap, onCancel, onCompare }: {
  pair: [Inspection, Inspection]; busy: boolean; closing: boolean; error: string;
  status: string; onChoose: (version: "old" | "new") => void;
  onSwap: () => void; onCancel: () => void; onCompare: () => void;
}) {
  const previous = useRef(document.activeElement === document.body
    ? document.querySelector<HTMLElement>('[data-action="open"]') : document.activeElement);
  useEffect(() => () => {
    requestAnimationFrame(() => {
      if (previous.current instanceof HTMLElement && previous.current.isConnected) previous.current.focus();
    });
  }, []);
  return <Modal.Backdrop isOpen isDismissable={!busy} isKeyboardDismissDisabled={busy} onOpenChange={(open) => { if (!open && !busy) onCancel(); }}>
    <Modal.Container size="lg"><Modal.Dialog>
      <Modal.Header><Modal.Heading>Compare score versions</Modal.Heading><Description>Confirm which score is old and which is new.</Description></Modal.Header>
      <Modal.Body>
        <DiffFileSelectors older={pair[0]} newer={pair[1]} disabled={busy || closing} onChoose={onChoose} onSwap={onSwap} />
        {error && <p role="alert" className="mt-4 text-danger">{error}</p>}
        {!error && status && <Description role="status" className="mt-4 whitespace-pre-line">{status}</Description>}
      </Modal.Body>
      <Modal.Footer><Button variant="secondary" data-action="cancel-diff" isDisabled={busy} onPress={onCancel}>Cancel</Button><Button data-action="compare-diff" isDisabled={busy || closing} onPress={onCompare}>{busy ? "Exporting…" : "Export diff"}</Button></Modal.Footer>
    </Modal.Dialog></Modal.Container>
  </Modal.Backdrop>;
}
