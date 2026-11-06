import { mount } from 'svelte';
import App from './App.svelte';
import { installFrontendErrorHandlers } from './lib/frontend-errors';

installFrontendErrorHandlers();

const app = mount(App, { target: document.getElementById('app')! });

export default app;
