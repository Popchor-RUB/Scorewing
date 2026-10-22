import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const linuxRuntime = /^(lib(c|m|dl|pthread|rt)\.so\.\d+|libgcc_s\.so\.1|ld-linux[^/]*\.so\.[12])$/;
const linuxDesktop = /^(lib(gdk-3|gdk_pixbuf-2\.0|cairo|gobject-2\.0|glib-2\.0|dbus-1|webkit2gtk-4\.1|gtk-3|soup-3\.0|gio-2\.0|javascriptcoregtk-4\.1)\.so\.\d+)$/;
const windowsRuntime = new Set([
  "advapi32", "bcrypt", "bcryptprimitives", "cfgmgr32", "combase", "comctl32", "comdlg32",
  "crypt32", "d2d1", "d3d11", "dcomp", "dwmapi", "dwrite", "dxgi", "gdi32",
  "imm32", "iphlpapi", "kernel32", "msimg32",
  "ncrypt", "ntdll", "ole32", "oleaut32", "opengl32", "powrprof", "propsys",
  "psapi", "rpcrt4", "secur32", "setupapi", "shcore", "shell32", "shlwapi", "user32",
  "userenv", "uxtheme", "version", "windowscodecs", "winhttp", "winmm", "winspool", "ws2_32",
].map((name) => `${name}.dll`));

export function checkDependencies(platform, output, desktop = false) {
  let dependencies;
  switch (platform) {
    case "darwin":
      dependencies = [...output.matchAll(/^\s+(.+?) \(compatibility version /gm)].map((m) => m[1]);
      break;
    case "linux":
      dependencies = [...output.matchAll(/\(NEEDED\).*Shared library: \[([^\]]+)\]/g)].map((m) => m[1]);
      break;
    case "win32":
      dependencies = [...output.matchAll(/^\s+Name: ([^\r\n]+?\.dll)\s*$/gmi)].map((m) => m[1]);
      break;
    default:
      throw new Error(`Unsupported platform: ${platform}`);
  }
  if (!dependencies.length && !(platform === "linux" && /There is no dynamic section/.test(output))) {
    throw new Error("No dependencies parsed; refusing to accept an unknown inspection format");
  }
  const unexpected = dependencies.filter((name) => {
    switch (platform) {
      case "darwin":
        return (!name.startsWith("/System/Library/") && !name.startsWith("/usr/lib/"))
          || /libxml2[^/]*\.dylib$/.test(name);
      case "linux":
        // GTK/WebKit are the desktop platform runtime; libxml2 must be static in our code.
        return !linuxRuntime.test(name) && !(desktop && linuxDesktop.test(name));
      case "win32":
        return !windowsRuntime.has(name.toLowerCase()) && !/^(api|ext)-ms-win-/i.test(name);
    }
  });
  if (unexpected.length) {
    throw new Error(`Unexpected shared dependencies: ${unexpected.join(", ")}`);
  }
  return dependencies;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const desktop = process.argv.includes("--desktop");
    const paths = process.argv.slice(2).filter((arg) => arg !== "--desktop");
    if (!paths.length) throw new Error("Usage: check-linkage.mjs [--desktop] BINARY...");
    for (const path of paths) {
      const [command, args] = {
        darwin: ["otool", ["-L", path]],
        linux: ["readelf", ["-d", path]],
        win32: [join(process.env.ProgramFiles ?? "C:/Program Files", "LLVM/bin/llvm-readobj.exe"), ["--coff-imports", path]],
      }[process.platform] ?? [];
      if (!command) throw new Error(`Unsupported platform: ${process.platform}`);
      const result = spawnSync(command, args, { encoding: "utf8", env: { ...process.env, LC_ALL: "C" } });
      if (result.error) throw result.error;
      if (result.status !== 0) throw new Error(`${command} failed: ${result.stderr}`);
      checkDependencies(process.platform, result.stdout, desktop);
      console.log(`${path}: shared dependencies limited to the platform runtime`);
    }
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
