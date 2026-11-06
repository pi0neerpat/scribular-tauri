import { mount } from 'svelte';
import App from './App.svelte';
import { openExternalLink } from './lib/api';

document.addEventListener('click', (event) => {
  const anchor = (event.target as Element | null)?.closest('a[href]') as HTMLAnchorElement | null;
  if (!anchor) return;
  const url = new URL(anchor.href, location.href);
  if (url.protocol === 'http:' || url.protocol === 'https:' || url.protocol === 'mailto:') {
    event.preventDefault();
    void openExternalLink(url.href);
  }
});

const app = mount(App, { target: document.getElementById('app')! });

export default app;
