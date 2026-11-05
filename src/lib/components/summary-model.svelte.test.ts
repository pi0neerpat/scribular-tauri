// @vitest-environment jsdom
// Parity spec 03 AS19: the model list and the "Use this model" button.
import { afterEach, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

const spec = (id: string, size: number, caution: string | null) => ({
  id, kind: 'llm', displayName: id, fileName: `${id}.gguf`, url: '', size, sha256: '',
  license: 'Gemma', licenseUrl: 'https://ai.google.dev/gemma/terms', optional: false,
  description: `${id} description`, caution,
});
const models = [
  { spec: spec('small', 1_600_000_000, null), selected: true, state: 'ready' },
  { spec: spec('big', 2_500_000_000, 'Needs about 8 GB of free memory.'), selected: false, state: 'missing' },
];

const setSummaryModel = vi.fn();
const openExternalLink = vi.fn();
vi.mock('../api', () => ({
  listSummaryModels: vi.fn(async () => models), onModelsProgress: vi.fn(async () => () => {}),
  openExternalLink, setSummaryModel,
}));
const { default: SummaryModel } = await import('./SummaryModel.svelte');

afterEach(() => vi.clearAllMocks());

it('shows size, caution, licence and Installed, and disables Use for the saved model', async () => {
  const target = document.createElement('div');
  document.body.append(target); // jsdom fires change only on connected inputs
  const app = mount(SummaryModel, { target });
  await vi.waitFor(() => expect(target.textContent).toContain('1.6 GB'));
  const text = target.textContent!;
  expect(text).toContain('2.5 GB');
  expect(text).toContain('Needs about 8 GB of free memory.');
  expect(text.match(/Installed/g)).toHaveLength(1);
  const use = [...target.querySelectorAll('button')].find((b) => b.textContent?.includes('Use this model'))!;
  expect(use.disabled).toBe(true);

  target.querySelector<HTMLButtonElement>('button')!.click();
  expect(openExternalLink).toHaveBeenCalledWith('https://ai.google.dev/gemma/terms');

  const big = target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1];
  big.click();
  flushSync();
  expect(use.disabled).toBe(false);
  use.click();
  flushSync();
  expect(target.textContent).toContain('Downloads 2.5 GB once.');
  expect(setSummaryModel).not.toHaveBeenCalled();
  unmount(app);
  target.remove();
});
