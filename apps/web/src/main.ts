import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';
import { startTheme } from './lib/theme';
import { startDebugBridge } from './lib/debug-bridge';

// A service worker from an earlier experiment would serve stale files; never keep one in dev.
if (import.meta.env.DEV && 'serviceWorker' in navigator) {
  navigator.serviceWorker.getRegistrations().then((rs) => rs.forEach((r) => r.unregister()));
}

// Test builds of the desktop app: the page half of the debug bridge, started first so it hears the whole log.
if (import.meta.env.VITE_UPDATE_CHANNEL === 'dev' && '__TAURI_INTERNALS__' in window) startDebugBridge();

// The saved theme is applied before anything is drawn, so there is no flash of the wrong one.
startTheme();

mount(App, { target: document.getElementById('app')! });
