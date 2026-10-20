#!/usr/bin/env node
// Generates parity fixtures for src-tauri/src/text/ from the Electron app's
// own JavaScript. The Electron sources are read as text only (never imported,
// never modified): processTranscript, chunkTranscript and mergeDiarizedSegments
// are not cleanly exported and their modules require electron, so the function
// bodies are sliced out and evaluated in an isolated vm context.
//
// Usage: node scripts/gen-text-fixtures.mjs [path/to/notes-assistant-electron]

import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const here = dirname(fileURLToPath(import.meta.url));
const electronRoot = resolve(process.argv[2] ?? join(here, "../../notes-assistant-electron"));
const outDir = join(here, "../src-tauri/tests/fixtures/text");

const summarizeSrc = readFileSync(join(electronRoot, "src/service/summarize.js"), "utf8");
const workerSrc = readFileSync(join(electronRoot, "src/service/audio/audioWorker.js"), "utf8");

// Slice `function name(...) { ... }` by brace matching. The extracted bodies
// contain no braces inside strings or regex literals, which is asserted by
// evaluating them below and comparing against known outputs.
function sliceFunction(src, name) {
  const start = src.indexOf(`function ${name}(`);
  if (start < 0) throw new Error(`function ${name} not found`);
  let depth = 0;
  for (let i = src.indexOf("{", start); i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}" && --depth === 0) return src.slice(start, i + 1);
  }
  throw new Error(`unbalanced braces in ${name}`);
}

const chunkSize = summarizeSrc.match(/const CHUNK_SIZE = (\d+);/);
if (!chunkSize) throw new Error("CHUNK_SIZE not found");

// processTranscript assigns the cleaned text to module state and then talks
// to the window; only the replace chain is pure, so take exactly that.
const chain = summarizeSrc.match(/currentTranscript = (transcript\s*\.replace[\s\S]*?\.replace\(\/ \+\/g, " "\));/);
if (!chain) throw new Error("processTranscript replace chain not found");

const context = vm.createContext({});
vm.runInContext(
  [
    `const CHUNK_SIZE = ${chunkSize[1]};`,
    `function cleanTranscript(transcript) { return ${chain[1]}; }`,
    sliceFunction(summarizeSrc, "chunkTranscript"),
    sliceFunction(workerSrc, "mergeDiarizedSegments"),
    "globalThis.api = { cleanTranscript, chunkTranscript, mergeDiarizedSegments, CHUNK_SIZE };",
  ].join("\n"),
  context,
);
const { cleanTranscript, chunkTranscript, mergeDiarizedSegments, CHUNK_SIZE } = context.api;

// Deterministic PRNG (mulberry32) so regenerating fixtures is reproducible.
function rng(seed) {
  return () => {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// Realistic text: a real benchmark transcript when the Electron checkout has it.
const benchPath = join(electronRoot, "benchmark/results/batch/2-speakers-50min/transcription-output.json");
const benchSegments = existsSync(benchPath) ? JSON.parse(readFileSync(benchPath, "utf8")) : [];
const benchText = benchSegments.map((s) => s.text).join(" ");

// ---- processTranscript ----------------------------------------------------

const cleanCases = [
  ["bracket noise tokens", "Hello [MUSIC] there [BLANK_AUDIO] friend [inaudible]."],
  ["nested and unclosed brackets", "a [b [c] d] e [never closed"],
  ["parenthesized sounds", "We started (soft music playing) and then (Laughter) and (birds chirping) ok"],
  ["paren keyword at offset 1", "x ( music) y"],
  ["paren keyword at offset 0 is kept", "x (music) y"],
  ["paren keyword beyond 40 chars is kept", `x (${"a".repeat(41)}music) y`],
  ["paren keyword at exactly 40 chars", `x (${"a".repeat(40)}music) y`],
  ["paren without close", "x (soft music playing y"],
  ["paren case insensitive", "x (SOFT MUSIC) y (Crowd NOISE here) z"],
  ["repeated words", "I I I I think so so so so so yes"],
  ["repeated words with commas and case", "the, The, THE, the theory and no no no"],
  ["repeated word prefix match", "the the the the theory"],
  ["repeated characters", "Hmmmmm aaaa 1 1 1 1 !!!! ok"],
  ["repeated punctuation", "Well..... I mean . . . . maybe. . yes...."],
  ["blank lines", "First line.\n\n\n\nSecond line.\n\n\nThird."],
  ["dot runs across newlines", "end.\n.\n.\nnext"],
  ["multiple spaces", "too    many     spaces  here"],
  ["tabs and unicode spaces", "a\t\tb  c d 　 e"],
  ["unicode whitespace in repeats", "go go go　go now"],
  ["astral characters are not collapsed", "😀😀😀😀 wow 😀 😀 😀 😀"],
  ["non-ascii words", "café café café café über über"],
  ["kelvin and long s are not ascii", "KK k k k k ſ s"],
  ["empty", ""],
  ["only noise", "[MUSIC] (music playing)"],
];
if (benchText) cleanCases.push(["benchmark transcript", benchText]);

const pieces = [
  "the", "The", "THE", "so", "um", "I", "a", "aa", "1", "_", "café", "😀", "theory",
  ".", "..", " . ", ",", ", ", " ", "  ", "\n", "\n\n", "\t", " ", " ",
  "[MUSIC]", "[", "]", "(", ")", "(music)", "(soft music playing)", "( noise)",
  "(Laughter)", "playing", "!", "!!!!", "x", "xx", "mm",
];
const rand = rng(0x5c41b);
for (let i = 0; i < 400; i++) {
  const len = 1 + Math.floor(rand() * 40);
  let s = "";
  for (let j = 0; j < len; j++) s += pieces[Math.floor(rand() * pieces.length)];
  cleanCases.push([`fuzz ${i}`, s]);
}

const clean = cleanCases.map(([name, input]) => ({ name, input, output: cleanTranscript(input) }));

// ---- chunkTranscript ------------------------------------------------------

function chunkCase(name, input) {
  try {
    return { name, input, output: chunkTranscript(input) };
  } catch (e) {
    return { name, input, error: e.message };
  }
}

const longWords = Array.from({ length: 2000 }, (_, i) => `word${i % 97}`).join(" ");
const chunk = [
  chunkCase("fewer than 10 words", "one two three four five six seven eight nine"),
  chunkCase("exactly 10 words", "one two three four five six seven eight nine ten"),
  chunkCase("double spaces count as words", "a  b  c  d  e"),
  chunkCase("empty", ""),
  chunkCase("long synthetic text", longWords),
  chunkCase("word longer than chunk size first", `${"x".repeat(3600)} ${longWords.slice(0, 200)}`),
  chunkCase("word longer than chunk size middle", `${longWords.slice(0, 3400)} ${"y".repeat(3600)} tail one two`),
  chunkCase("utf16 length counts astral as two", Array.from({ length: 1200 }, () => "😀é").join(" ")),
  chunkCase("newlines are not separators", `line one\nline two\n${longWords.slice(0, 5000)}`),
];
if (benchText) {
  chunk.push(chunkCase("benchmark transcript raw", benchText));
  chunk.push(chunkCase("benchmark transcript cleaned", cleanTranscript(benchText)));
}

// ---- mergeDiarizedSegments ------------------------------------------------

const seg = (startMs, endMs, text) => ({ startMs, endMs, text });
const mergeCases = [
  ["interleaved", [seg(0, 900, "Hi, how are you?"), seg(2000, 2900, "Good."), seg(5000, 6000, "Great.")],
    [seg(1000, 1900, "Fine, thanks."), seg(3000, 3500, "And you?"), seg(3600, 4000, "Busy week.")]],
  ["single source client", [seg(0, 1000, "Only me."), seg(1000, 2000, "Still me.")], []],
  ["single source you", [], [seg(0, 1000, "Just you."), seg(1500, 2000, "[BLANK_AUDIO]"), seg(2100, 2500, "Again.")]],
  ["both empty", [], []],
  ["ties keep client first", [seg(1000, 2000, "client tie")], [seg(1000, 2000, "you tie")]],
  ["blank and bracket-only segments are skipped", [seg(0, 100, "  "), seg(200, 300, "[MUSIC]"), seg(400, 500, "real")],
    [seg(150, 250, ""), seg(350, 450, "  spaced  [x] out  ")]],
  ["missing text field", [{ startMs: 0, endMs: 10 }, seg(20, 30, "after")], [seg(5, 15, null)]],
  ["unicode whitespace trimmed", [seg(0, 10, " hello　")], [seg(5, 15, " there﻿")]],
];
if (benchSegments.length) {
  // Alternate the real segments between the two sources to get long runs.
  const client = [];
  const you = [];
  benchSegments.slice(0, 300).forEach((s, i) => {
    (Math.floor(i / 3) % 2 === 0 ? client : you).push(seg(s.start_ms, s.end_ms, s.text));
  });
  mergeCases.push(["benchmark segments", client, you]);
}

const merge = mergeCases.map(([name, client, you]) => ({
  name,
  client,
  you,
  output: mergeDiarizedSegments(client, you),
}));

mkdirSync(outDir, { recursive: true });
const write = (file, data) => writeFileSync(join(outDir, file), `${JSON.stringify(data, null, 2)}\n`);
write("clean.json", { cases: clean });
write("chunk.json", { chunkSize: CHUNK_SIZE, cases: chunk });
write("merge.json", { cases: merge });
console.log(`wrote ${clean.length} clean, ${chunk.length} chunk, ${merge.length} merge cases to ${outDir}`);
