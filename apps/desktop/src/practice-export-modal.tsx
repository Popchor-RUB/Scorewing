import { useState } from "react";
import { Button, Description, Modal } from "@heroui/react";
import { invoke } from "@tauri-apps/api/core";
import type { PracticeResult } from "./model.ts";

export function PracticeExportModal({ result, onClose }: { result: PracticeResult; onClose: () => void }) {
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState("");

  async function openFolder() {
    if (opening) return;
    setOpening(true);
    setError("");
    try { await invoke("open_practice_export_folder", { directory: result.directory }); }
    catch (error) { setError(`Could not open the folder: ${String(error)}`); }
    finally { setOpening(false); }
  }

  return <Modal.Backdrop isOpen onOpenChange={(open) => { if (!open) onClose(); }}>
    <Modal.Container size="md" scroll="inside">
      <Modal.Dialog>
        <Modal.CloseTrigger aria-label="Close practice track completion" />
        <Modal.Header>
          <svg className="practice-export-hero w-full text-accent" aria-hidden="true" focusable="false" viewBox="0 0 360 160">
            <ellipse cx="180" cy="137" rx="118" ry="9" fill="currentColor" opacity=".08" />
            <g fill="none" stroke="currentColor" opacity=".16" strokeWidth="2">
              <path d="M35 62h290M35 76h290M35 90h290M35 104h290M35 118h290" />
            </g>
            <g className="dancing-note" fill="currentColor">
              <ellipse cx="92" cy="103" rx="13" ry="9" transform="rotate(-20 92 103)" />
              <path d="M101 101V43h5c0 17 24 14 17 38-2-13-13-13-17-18v38Z" />
            </g>
            <g className="dancing-note" fill="currentColor">
              <ellipse cx="170" cy="106" rx="13" ry="9" transform="rotate(-20 170 106)" />
              <ellipse cx="215" cy="95" rx="13" ry="9" transform="rotate(-20 215 95)" />
              <path d="M179 105V40l49-12v66h-5V46l-39 10v49Z" />
            </g>
            <g className="dancing-note" fill="currentColor">
              <ellipse cx="275" cy="105" rx="12" ry="9" transform="rotate(-20 275 105)" />
              <path d="M283 104V46h5v58Z" />
            </g>
            <g fill="currentColor" opacity=".4">
              <path d="m56 35 2 6 6 2-6 2-2 6-2-6-6-2 6-2Zm246-13 2 6 6 2-6 2-2 6-2-6-6-2 6-2Z" />
              <circle cx="143" cy="26" r="3" />
            </g>
          </svg>
          <Modal.Heading>Practice tracks created</Modal.Heading>
          <Description>{result.completed.length} completed {result.completed.length === 1 ? "track" : "tracks"} saved in:</Description>
        </Modal.Header>
        <Modal.Body>
          <p className="practice-export-path select-text wrap-anywhere rounded-xl bg-default p-3 text-sm">{result.directory}</p>
          {error && <p className="mt-3 text-danger" role="alert">{error}</p>}
        </Modal.Body>
        <Modal.Footer>
          <Button variant="secondary" data-action="practice-export-done" onPress={onClose}>Done</Button>
          <Button variant="primary" data-action="practice-open-folder" isDisabled={opening} onPress={() => void openFolder()}>
            {opening ? "Opening…" : "Open folder"}
          </Button>
        </Modal.Footer>
      </Modal.Dialog>
    </Modal.Container>
  </Modal.Backdrop>;
}
