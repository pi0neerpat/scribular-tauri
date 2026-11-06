import { reportFrontendError } from './api';

export function framesFromStack(stack?: string): string[] {
  if (!stack) return [];
  return stack.split('\n').flatMap((line) => {
    const match = line.match(/(?:\(|\s)(https?:\/\/[^\s)]+|tauri:\/\/[^\s)]+|[^\s()]+):(\d+):(\d+)\)?$/);
    if (!match) return [];
    try {
      const url = new URL(match[1]);
      if (!(url.protocol === 'tauri:' || url.hostname === 'tauri.localhost' || url.hostname === 'localhost')) return [];
      return [`${url.pathname.replace(/^\//, '')}:${match[2]}:${match[3]}`];
    } catch { return []; }
  }).slice(0, 8);
}

export function installFrontendErrorHandlers() {
  window.onerror = (_message, _source, _line, _column, error) => {
    void reportFrontendError('window_error', framesFromStack(error?.stack)).catch(() => {});
  };
  window.onunhandledrejection = (event) => {
    void reportFrontendError('unhandled_rejection', framesFromStack(event.reason instanceof Error ? event.reason.stack : undefined)).catch(() => {});
  };
}
