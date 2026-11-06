import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

const { installFrontendErrorHandlers } = await import('./frontend-errors');

describe('frontend error reporting', () => {
  beforeEach(() => {
    invoke.mockReset().mockResolvedValue(undefined);
    Object.defineProperty(globalThis, 'window', { configurable: true, value: {} });
    installFrontendErrorHandlers();
  });

  it('sends only a closed kind and origin-free frames', async () => {
    const error = new Error('CANARY-MESSAGE');
    error.name = 'CANARY-NAME';
    error.stack = 'Error: CANARY-MESSAGE\n    at call (http://tauri.localhost/src/app.js:12:4)';
    window.onerror?.('CANARY-MESSAGE', 'http://tauri.localhost/src/app.js', 12, 4, error);
    await Promise.resolve();
    expect(invoke).toHaveBeenCalledWith('report_frontend_error', {
      kind: 'window_error', frames: ['src/app.js:12:4'],
    });
    expect(JSON.stringify(invoke.mock.calls)).not.toContain('CANARY');
  });
});
