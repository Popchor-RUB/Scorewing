import { useEffect } from "react";
import { Button, Description, Label, Modal } from "@heroui/react";
import type { Inspection } from "./model.ts";

export function DiffPairModal({ pair, busy, closing, error, onSwap, onCancel, onCompare }: {
  pair: [Inspection, Inspection]; busy: boolean; closing: boolean; error: string;
  onSwap: () => void; onCancel: () => void; onCompare: () => void;
}) {
  useEffect(() => {
    const previous = document.activeElement;
    return () => { if (previous instanceof HTMLElement && previous.isConnected) previous.focus(); };
  }, []);
  return <Modal.Backdrop isOpen isDismissable={!busy} isKeyboardDismissDisabled={busy} onOpenChange={(open) => { if (!open && !busy) onCancel(); }}>
    <Modal.Container size="lg"><Modal.Dialog>
      <Modal.Header><Modal.Heading>Compare score versions</Modal.Heading><Description>Confirm which score is old and which is new.</Description></Modal.Header>
      <Modal.Body>
        <div className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-4">
          <div data-version="old"><Label>Old</Label><p className="wrap-anywhere">{pair[0].name}</p></div>
          <Button isIconOnly variant="secondary" aria-label="Swap old and new files" data-action="swap-diff" isDisabled={busy || closing} onPress={onSwap}><span aria-hidden="true">⇄</span></Button>
          <div data-version="new"><Label>New</Label><p className="wrap-anywhere">{pair[1].name}</p></div>
        </div>
        {error && <p role="alert" className="mt-4 text-danger">{error}</p>}
      </Modal.Body>
      <Modal.Footer><Button variant="secondary" data-action="cancel-diff" isDisabled={busy} onPress={onCancel}>Cancel</Button><Button data-action="compare-diff" isDisabled={busy || closing} onPress={onCompare}>{busy ? "Preparing scores…" : "Compare"}</Button></Modal.Footer>
    </Modal.Dialog></Modal.Container>
  </Modal.Backdrop>;
}
