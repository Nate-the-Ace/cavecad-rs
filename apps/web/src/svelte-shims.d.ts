declare module '*.svelte' {
  import type { Component } from 'svelte';
  const component: Component<any>;
  export default component;
}

declare module 'dockview-core/dist/dockview-core.js' {
  export * from 'dockview-core';
}
