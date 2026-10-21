import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { basename } from "node:path";

for (const path of process.argv.slice(2)) {
  const hash = createHash("sha256").update(readFileSync(path)).digest("hex");
  writeFileSync(`${path}.sha256`, `${hash}  ${basename(path)}\n`);
}
