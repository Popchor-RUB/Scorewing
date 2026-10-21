import assert from "node:assert/strict";
import test from "node:test";
import { checkDependencies } from "./check-linkage.mjs";

const macho = (name) => `app:\n\t${name} (compatibility version 1.0.0, current version 1.0.0)\n`;
const elf = (name) => ` 0x0000000000000001 (NEEDED) Shared library: [${name}]\n`;
const pe = (name) => `Import {\n  Name: ${name}\n}\n`;

test("macOS allows OS frameworks and rejects libxml2 and package manager paths", () => {
  checkDependencies("darwin", macho("/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit"));
  checkDependencies("darwin", macho("/usr/lib/libSystem.B.dylib"));
  for (const name of ["/opt/homebrew/opt/libxml2/lib/libxml2.16.dylib", "/Volumes/Macintosh HD/lib/libxml2.dylib", "/usr/local/lib/libicuuc.dylib", "/usr/lib/libxml2.2.dylib", "@rpath/libxml2.dylib"]) {
    assert.throws(() => checkDependencies("darwin", macho(name)), /Unexpected shared/);
  }
});

test("Linux permits GTK/WebKit only for the desktop and rejects external libraries", () => {
  checkDependencies("linux", elf("libc.so.6") + elf("libgcc_s.so.1"));
  checkDependencies("linux", "There is no dynamic section in this file.");
  checkDependencies("linux", elf("libwebkit2gtk-4.1.so.0"), true);
  assert.throws(() => checkDependencies("linux", elf("libwebkit2gtk-4.1.so.0")), /Unexpected shared/);
  for (const name of ["libxml2.so.2", "libxml2.so.16", "libicuuc.so.76", "libz.so.1", "liblzma.so.5", "libunknown.so.1"]) {
    assert.throws(() => checkDependencies("linux", elf(name), true), /Unexpected shared/);
  }
});

test("Windows allows OS DLLs and rejects application DLLs and dynamic VC runtimes", () => {
  checkDependencies("win32", pe("KERNEL32.dll") + pe("api-ms-win-core-synch-l1-2-0.dll"));
  for (const name of ["libxml2.dll", "zlib1.dll", "iconv-2.dll", "VCRUNTIME140.dll", "MSVCP140.dll", "unknown.dll", "some library.dll"]) {
    assert.throws(() => checkDependencies("win32", pe(name)), /Unexpected shared/);
  }
});

test("unknown or empty tool output fails closed", () => {
  for (const platform of ["darwin", "linux", "win32"]) {
    assert.throws(() => checkDependencies(platform, ""), /No dependencies parsed/);
    assert.throws(() => checkDependencies(platform, "unexpected output"), /No dependencies parsed/);
  }
  assert.throws(() => checkDependencies("unsupported", ""), /Unsupported platform/);
});
