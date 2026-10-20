#!/usr/bin/env node
// Copies the 7 default prompts word for word from the Electron app's
// `src/store/defaults.js` into src-tauri/src/store/default_prompts.json, which
// the store's seed migration embeds. The Electron source is read as text and
// evaluated in an isolated vm context with `require` stubbed out, so nothing
// from the Electron app is imported or modified.
//
// Usage: node scripts/gen-default-prompts.mjs [path/to/notes-assistant-electron]

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const here = dirname(fileURLToPath(import.meta.url));
const electronRoot = resolve(process.argv[2] ?? join(here, "../../notes-assistant-electron"));
const outFile = join(here, "../src-tauri/src/store/default_prompts.json");

const src = readFileSync(join(electronRoot, "src/store/defaults.js"), "utf8");
const module = { exports: {} };
// defaults.js only requires models.js for DEFAULT_MODEL_ID, which we drop.
vm.runInContext(src, vm.createContext({ module, require: () => ({}) }));
const defaults = module.exports;

const prompts = defaults.prompts.map(({ id, name, description, text, isDefault, starred }) => ({
  id,
  name,
  description,
  text,
  isDefault,
  starred,
}));
if (prompts.length !== 7) throw new Error(`expected 7 prompts, found ${prompts.length}`);
if (!prompts.some((p) => p.id === defaults.activePromptId)) {
  throw new Error(`activePromptId ${defaults.activePromptId} is not a prompt`);
}

writeFileSync(
  outFile,
  JSON.stringify({ activePromptId: defaults.activePromptId, prompts }, null, 2) + "\n",
);
console.log(`wrote ${prompts.length} prompts to ${outFile}`);
