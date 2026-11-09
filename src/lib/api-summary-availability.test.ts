import { afterEach, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));
const { subscribeSummaryAvailability } = await import('./api');

afterEach(() => vi.clearAllMocks());

it('reads the snapshot after the listener is registered', async () => {
  const order: string[] = [];
  mocks.listen.mockImplementation(async (_event: string, _handler: unknown) => {
    order.push('listen');
    return () => {};
  });
  mocks.invoke.mockImplementation(async () => { order.push('snapshot'); return 'ready'; });
  const state = vi.fn();

  await subscribeSummaryAvailability(state);

  expect(order).toEqual(['listen', 'snapshot']);
  expect(state).toHaveBeenCalledWith('ready');
});

it('keeps an event that arrives while the snapshot is pending', async () => {
  let emit!: (event: { payload: string }) => void;
  mocks.listen.mockImplementation(async (_event: string, handler: (event: { payload: string }) => void) => {
    emit = handler;
    return () => {};
  });
  let resolveSnapshot!: (state: string) => void;
  mocks.invoke.mockReturnValue(new Promise((resolve) => { resolveSnapshot = resolve; }));
  const state = vi.fn();

  const subscription = subscribeSummaryAvailability(state);
  await Promise.resolve();
  emit({ payload: 'ready' });
  resolveSnapshot('downloading');
  await subscription;

  expect(state).toHaveBeenCalledOnce();
  expect(state).toHaveBeenCalledWith('ready');
});

it('unlistens and propagates a failed availability snapshot', async () => {
  const unlisten = vi.fn();
  mocks.listen.mockResolvedValue(unlisten);
  mocks.invoke.mockRejectedValue(new Error('IPC failed'));

  await expect(subscribeSummaryAvailability(vi.fn())).rejects.toThrow('IPC failed');
  expect(unlisten).toHaveBeenCalledOnce();
});

it('propagates a failed listener setup', async () => {
  mocks.listen.mockRejectedValue(new Error('listener failed'));

  await expect(subscribeSummaryAvailability(vi.fn())).rejects.toThrow('listener failed');
  expect(mocks.invoke).not.toHaveBeenCalled();
});
