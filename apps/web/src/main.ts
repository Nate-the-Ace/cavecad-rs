import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';

// A service worker from an earlier experiment would serve stale files; never keep one in dev.
if (import.meta.env.DEV && 'serviceWorker' in navigator) {
  navigator.serviceWorker.getRegistrations().then((rs) => rs.forEach((r) => r.unregister()));
}

mount(App, { target: document.getElementById('app')! });
