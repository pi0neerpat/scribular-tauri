import { readFile, writeFile } from 'node:fs/promises';

export function makeLatestJson({ version, notes, signature, url, pubDate = new Date().toISOString() }) {
  if (!version || !notes || !signature || !url || !url.startsWith('https://')) throw new Error('version, notes, signature and an https URL are required');
  return { version, notes, pub_date: new Date(pubDate).toISOString(), platforms: { 'darwin-aarch64': { signature, url } } };
}

if (process.argv[1] && new URL(import.meta.url).pathname === process.argv[1]) {
  try {
    const [version, notes, signaturePath, url, outputPath] = process.argv.slice(2);
    if (!outputPath) throw new Error('usage: make-latest-json.mjs VERSION NOTES SIGNATURE_FILE URL OUTPUT_FILE');
    const signature = (await readFile(signaturePath, 'utf8')).trim();
    await writeFile(outputPath, `${JSON.stringify(makeLatestJson({ version, notes, signature, url }), null, 2)}\n`);
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
