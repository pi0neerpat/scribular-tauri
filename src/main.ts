import { mount } from 'svelte';
import App from './App.svelte';
import { openExternalLink } from './lib/api';

document.addEventListener('click', (event) => {
  const anchor = event.target instanceof Element ? event.target.closest('a[href]') as HTMLAnchorElement | null : null;
  if (!anchor) return;
  if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  const url = new URL(anchor.href, location.href);
  if (url.protocol === 'mailto:' || ((url.protocol === 'http:' || url.protocol === 'https:') && url.origin !== location.origin)) {
    event.preventDefault();
    void openExternalLink(url.href).catch((error) => console.error('Failed to open external link', error));
  }
});

const app = mount(App, { target: document.getElementById('app')! });

export default app;
