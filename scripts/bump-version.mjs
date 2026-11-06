import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

export async function bumpVersion(kind, root = process.cwd()) {
  if (!['patch', 'minor', 'major'].includes(kind)) throw new Error('bump must be patch, minor, or major');
  const cargoPath = resolve(root, 'src-tauri/Cargo.toml');
  const tauriPath = resolve(root, 'src-tauri/tauri.conf.json');
  const packagePath = resolve(root, 'package.json');
  const cargo = await readFile(cargoPath, 'utf8');
  const match = cargo.match(/^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"\s*$/m);
  if (!match) throw new Error('src-tauri/Cargo.toml has no package version');
  let [, major, minor, patch] = match.map(Number);
  if (kind === 'major') { major++; minor = 0; patch = 0; }
  else if (kind === 'minor') { minor++; patch = 0; }
  else patch++;
  const version = `${major}.${minor}.${patch}`;
  const tauri = JSON.parse(await readFile(tauriPath, 'utf8'));
  const pkg = JSON.parse(await readFile(packagePath, 'utf8'));
  tauri.version = version;
  pkg.version = version;
  await Promise.all([
    writeFile(cargoPath, cargo.replace(match[0], `version = "${version}"`)),
    writeFile(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`),
    writeFile(packagePath, `${JSON.stringify(pkg, null, 2)}\n`),
  ]);
  return version;
}

if (process.argv[1] && resolve(process.argv[1]) === new URL(import.meta.url).pathname) {
  try { console.log(await bumpVersion(process.argv[2])); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
