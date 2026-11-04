# Export names and practice destinations

Status: approved and implemented in the desktop application.

Editable source: [ScoreWing desktop Figcraft document](../desktop-redesign/scorewing-desktop.figcraft), page **05 · Export destinations**, frames 20–23. Screens were created and exported using the Figcraft MCP. Open [the gallery](index.html) to review.

## Export filenames

Use `[original name]-[toolName].[extension]` in the native save dialog:

| Tool | Example suggestion |
| --- | --- |
| Split | `A Million Dreams-split.musicxml` |
| Merge | `A Million Dreams-merge.musicxml` |
| Diff | `A Million Dreams-diff.mxl` |

Use the original source filename without its extension. Diff uses the newer source. Preserve the existing default output formats and supported extension choices; users can edit the suggestion. Converted MuseScore inputs still use the original filename. Continue protecting source scores against accidental replacement.

## Practice output

The folder picker chooses the actual export destination. Update its title to **Choose an output folder for practice tracks**. Place `00 - Whole song/` (when enabled) and numbered section folders directly inside it, using the existing folder and track naming rules. Do not add a score-named parent or numbered export copy. The completion modal and Open folder action refer to this selected destination.

Before rendering or changing existing output, calculate the intended destination files. If any would be replaced, show the existing-files modal with the collision count and full output path:

- **Overwrite matching files** is initially selected. The action reads **Overwrite & export**. Replace only files included in this export; preserve every other file and folder.
- **Clear folder before exporting** is an explicit alternative. Explain that it deletes everything inside the selected folder, including unrelated files and subfolders. Its action reads **Clear folder & export** and uses destructive styling.
- **Cancel**, Escape, and closing the modal leave existing output untouched. Focus starts on Cancel; restore focus when closing. Use the existing application-owned shadcn dialog, button, and radio controls where available, with the current system theme and responsive dialog sizing.

Do not offer a clear action that can delete an open source score. Validate destinations in the backend, prevent concurrent exports from bypassing confirmation, and recheck changed collisions before writing. Preserve existing cancellation, completed-track retention, renderer validation, and error reporting. A failure must never silently fall back to clearing a folder.

No-conflict exports proceed without this modal. A folder/file type mismatch is an actionable error rather than permission to delete an unexpected directory.

## Implementation and verification

The desktop save-dialog naming lives in `apps/desktop/src-tauri/src/lib.rs`. Practice destination planning and atomic publication live in `crates/practice-tracks/src/output.rs`; `service.rs` reuses the same folder and filename functions for planning and rendering. The existing export command retains its single-job guard while awaiting the conflict event's token-checked reply. The backend rechecks the destination and protects current and snapshotted sources under the document lock before clearing. The source-replacement check in `scorewing-core` is shared across MusicXML and MP3 exports.

The conflict dialog uses application-owned shadcn/Base UI dialog, button, and radio primitives. The existing progress, cancellation, partial-results, completion, and Open folder flows are retained. Successful completion lists the actual section folders. CLI behavior is preserved.

Validation:

- Inspected Figcraft layers and four 1100×760 PNG exports before approval.
- `cargo test --offline --workspace`: 217 passed; five existing native MuseScore/audio acceptance tests ignored.
- `npm test`: 55 passed. Added conflict-choice, reply-failure/retry, unsafe-clear, cancellation, and focus checks; verified direct section folders in the completion modal.
- `npm run build`: passed, including TypeScript. Vite reports the existing Base UI `use client` directive warnings.
- `cargo fmt --all` and `git diff --check`: passed.

Tests cover filenames from original stems, direct output, overwrite preservation, explicit clear and read-only cancellation, changed destinations, unexpected file types, symlink behavior, source/renderer protection, stale or duplicate confirmation replies, and existing parallel-render cancellation/failure behavior. Rust validation used the cached 1.90 toolchain and native library sysroot. No live MuseScore audio rendering or interactive desktop smoke test was performed for this change.
