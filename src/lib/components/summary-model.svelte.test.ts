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
let modelChanged: ((id: string) => void) | undefined;

const setSummaryModel = vi.fn();
const openExternalLink = vi.fn();
const listSummaryModels = vi.fn(async () => models);
const onSummaryModelChanged = vi.fn(async (callback: (id: string) => void) => { modelChanged = callback; return () => {}; });
let reportProgress: ((progress: { modelId: string; state: string; downloadedBytes: number; totalBytes: number; error: string | null }) => void) | undefined;
vi.mock('../api', () => ({
  listSummaryModels, onModelsProgress: vi.fn(async (callback) => { reportProgress = callback; return () => {}; }),
  onSummaryModelChanged,
  openExternalLink, setSummaryModel,
}));
const { default: SummaryModel } = await import('./SummaryModel.svelte');

afterEach(() => { vi.clearAllMocks(); modelChanged = undefined; });

it('applies progress locally instead of fetching the model list for every tick', async () => {
  const target = document.createElement('div');
  const app = mount(SummaryModel, { target });
  await vi.waitFor(() => expect(listSummaryModels).toHaveBeenCalledOnce());
  reportProgress!({ modelId: 'big', state: 'downloading', downloadedBytes: 1, totalBytes: 2_500_000_000, error: null });
  reportProgress!({ modelId: 'big', state: 'downloading', downloadedBytes: 2, totalBytes: 2_500_000_000, error: null });
  await vi.waitFor(() => expect(target.textContent).toContain('Downloading'));
  expect(listSummaryModels).toHaveBeenCalledOnce();
  unmount(app);
  target.remove();
});

it('restores the backend selection when switching fails', async () => {
  setSummaryModel.mockRejectedValueOnce(new Error('download failed'));
  const target = document.createElement('div');
  document.body.append(target);
  const app = mount(SummaryModel, { target });
  await vi.waitFor(() => expect(target.querySelectorAll('input[type="radio"]')).toHaveLength(2));
  const radios = target.querySelectorAll<HTMLInputElement>('input[type="radio"]');
  radios[1].click();
  flushSync();
  const use = [...target.querySelectorAll('button')].at(-1)!;
  use.click();
  flushSync();
  use.click();
  await vi.waitFor(() => expect(radios[0].checked).toBe(true));
  unmount(app);
  target.remove();
});

it('restores the backend selection after a download fails', async () => {
  const target = document.createElement('div');
  document.body.append(target);
  const app = mount(SummaryModel, { target });
  await vi.waitFor(() => expect(target.querySelectorAll('input[type="radio"]')).toHaveLength(2));
  const radios = target.querySelectorAll<HTMLInputElement>('input[type="radio"]');
  radios[1].click();
  flushSync();
  const use = [...target.querySelectorAll('button')].at(-1)!;
  use.click();
  flushSync();
  use.click();
  await vi.waitFor(() => expect(radios[1].checked).toBe(true));
  reportProgress!({ modelId: 'big', state: 'failed', downloadedBytes: 0, totalBytes: 2_500_000_000, error: 'network failed' });
  modelChanged!('small');
  await vi.waitFor(() => expect(radios[0].checked).toBe(true));
  expect(target.textContent).toContain('network failed');
  unmount(app);
  target.remove();
});

it('refreshes selection after another view changes the model', async () => {
  const target = document.createElement('div');
  const app = mount(SummaryModel, { target });
  await vi.waitFor(() => expect(target.textContent).toContain('1.6 GB'));
  modelChanged!('big');
  await vi.waitFor(() => expect(target.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1].checked).toBe(true));
  unmount(app);
  target.remove();
});

it('shows the selected model immediately after a successful switch', async () => {
  const target = document.createElement('div');
  document.body.append(target);
  const app = mount(SummaryModel, { target });
  await vi.waitFor(() => expect(target.textContent).toContain('1.6 GB'));
  const radios = target.querySelectorAll<HTMLInputElement>('input[type="radio"]');
  radios[1].click();
  flushSync();
  [...target.querySelectorAll('button')].at(-1)!.click();
  flushSync();
  [...target.querySelectorAll('button')].at(-1)!.click();
  await vi.waitFor(() => expect(radios[1].checked).toBe(true));
  expect(setSummaryModel).toHaveBeenCalledWith('big');
  unmount(app);
  target.remove();
});

it('shows errors when the licence link cannot open', async () => {
  openExternalLink.mockRejectedValueOnce(new Error('browser unavailable'));
  const target = document.createElement('div');
  document.body.append(target);
  const app = mount(SummaryModel, { target });
  await vi.waitFor(() => expect(target.textContent).toContain('1.6 GB'));
  target.querySelector<HTMLButtonElement>('button')!.click();
  expect(openExternalLink).toHaveBeenCalledWith('https://ai.google.dev/gemma/terms');
  await vi.waitFor(() => expect(target.textContent).toContain('browser unavailable'));
  unmount(app);
  target.remove();
});

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
