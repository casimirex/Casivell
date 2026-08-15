// Copies the freshly built engine into `public/`, so the Rust build and the
// browser build stay one command apart. Paths are resolved relative to this
// file, so it runs from anywhere.
import { copyFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", ".."); // workspace root
const src = join(root, "target", "wasm32-unknown-unknown", "release", "casivell_wasm.wasm");
const dst = join(root, "web-react", "public", "casivell_wasm.wasm");

if (!existsSync(src)) {
  console.error(`No built engine at ${src}`);
  console.error(
    "Build it first: cargo build --workspace --target wasm32-unknown-unknown --release",
  );
  process.exit(1);
}

copyFileSync(src, dst);
console.log(`Copied ${src}`);
console.log(`   -> ${dst}`);
