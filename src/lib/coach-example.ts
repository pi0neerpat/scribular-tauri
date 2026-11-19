export function exampleError(text: string): string | null {
  if (!text.trim()) return 'Enter an example note first.';
  const length = [...text].length;
  if (length < 100) return 'Use an example note with at least 100 characters.';
  if (length > 12_000) return 'Use an example note with at most 12,000 characters.';
  if (/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/u.test(text)) return 'This example contains characters that aren’t plain text.';
  return null;
}

export async function readExample(file: File): Promise<string> {
  if (!/\.(txt|md)$/i.test(file.name)) throw new Error('Choose a .txt or .md file.');
  if (file.size > 48_003) throw new Error('Use an example note with at most 12,000 characters.');
  let buffer: ArrayBuffer;
  try { buffer = await file.arrayBuffer(); }
  catch { throw new Error('I couldn’t read that file. Try another file.'); }
  let text: string;
  try { text = new TextDecoder('utf-8', { fatal: true }).decode(buffer); }
  catch { throw new Error('Choose a text file saved as UTF-8.'); }
  const error = exampleError(text);
  if (error) throw new Error(error);
  return text;
}
