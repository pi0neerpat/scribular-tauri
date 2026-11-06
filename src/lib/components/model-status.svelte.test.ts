// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { mount, unmount } from 'svelte';

let resolveList!: (value: { spec: { id: string }; selected: boolean }[]) => void;
let modelChanged: ((id: string) => void) | undefined;
let initialListResolved = false;
const listSummaryModels = vi.fn(() =>
  new Promise<{ spec: { id: string }; selected: boolean }[]>((resolve) => { resolveList = resolve; })
    .then((value) => { initialListResolved = true; return value; }),
);
const modelsStatus = vi.fn(async () => [
  { modelId: 'small', state: 'ready', downloadedBytes: 1, totalBytes: 1, error: null },
  { modelId: 'big', state: 'downloading', downloadedBytes: 50, totalBytes: 100, error: null },
]);
const onModelsProgress = vi.fn(async (_handler: (progress: never) => void) => () => {});
const onSummaryModelChanged = vi.fn(async (handler: (id: string) => void) => {
  modelChanged = handler;
  return () => {};
});
vi.mock('../api', () => ({
  downloadModels: vi.fn(), listSummaryModels, modelsStatus, onModelsProgress, onSummaryModelChanged,
}));
const { default: ModelStatus } = await import('./ModelStatus.svelte');

afterEach(() => { vi.clearAllMocks(); modelChanged = undefined; initialListResolved = false; });

it('keeps switch and rollback events newer than the initial model snapshot', async () => {
  const target = document.createElement('div');
  const app = mount(ModelStatus, { target });
  await vi.waitFor(() => expect(modelChanged).toBeTypeOf('function'));
  await vi.waitFor(() => expect(listSummaryModels).toHaveBeenCalledOnce());
  modelChanged!('big');
  await vi.waitFor(() => expect(target.textContent).toMatch(/Summary model downloading\s*\(50%\)/));

  resolveList([
    { spec: { id: 'small' }, selected: true },
    { spec: { id: 'big' }, selected: false },
  ]);
  await vi.waitFor(() => expect(initialListResolved).toBe(true));
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(target.textContent).toMatch(/Summary model downloading\s*\(50%\)/);

  modelChanged!('small');
  await vi.waitFor(() => expect(target.textContent).toContain('Summary model ready'));
  unmount(app);
  target.remove();
});

it('loads the status snapshot if progress listener setup fails', async () => {
  onModelsProgress.mockRejectedValueOnce(new Error('listener unavailable'));
  const target = document.createElement('div');
  const app = mount(ModelStatus, { target });
  await vi.waitFor(() => expect(listSummaryModels).toHaveBeenCalledOnce());
  resolveList([{ spec: { id: 'small' }, selected: true }]);
  await vi.waitFor(() => expect(target.textContent).toContain('Summary model ready'));
  expect(modelsStatus).toHaveBeenCalledOnce();
  unmount(app);
  target.remove();
});

it('handles a rejected model-change listener during setup and cleanup', async () => {
  onSummaryModelChanged.mockRejectedValueOnce(new Error('listener unavailable'));
  const target = document.createElement('div');
  const app = mount(ModelStatus, { target });
  await vi.waitFor(() => expect(listSummaryModels).toHaveBeenCalledOnce());
  resolveList([{ spec: { id: 'small' }, selected: true }]);
  await vi.waitFor(() => expect(target.textContent).toContain('Summary model ready'));
  unmount(app);
  target.remove();
});
