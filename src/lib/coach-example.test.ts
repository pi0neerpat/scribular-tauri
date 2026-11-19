import { expect, it } from 'vitest';
import { exampleError, readExample } from './coach-example';

it('counts Unicode characters and decodes only supported UTF-8 text files', async () => {
  const short = '𐐀'.repeat(100);
  const long = '𐐀'.repeat(12_000);
  expect(exampleError('')).toMatch(/Enter an example/);
  expect(exampleError(short.slice(2))).toMatch(/at least 100/);
  expect(exampleError(long + 'é')).toMatch(/at most 12,000/);
  expect(exampleError(short + '\0')).toMatch(/plain text/);
  await expect(readExample(new File([short], 'example.txt'))).resolves.toBe(short);
  await expect(readExample(new File([long], 'example.MD'))).resolves.toBe(long);
  await expect(readExample(new File([short], 'example.rtf'))).rejects.toThrow('.txt or .md');
  await expect(readExample(new File([new Uint8Array([0xc3, 0x28])], 'example.txt'))).rejects.toThrow('UTF-8');
});
