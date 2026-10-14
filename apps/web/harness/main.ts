import { mount } from 'svelte';
import '../src/app.css';
import { startTheme } from '../src/lib/theme';
import { cave } from '../src/lib/cave.svelte';
import Harness from './Harness.svelte';
import { overrides } from './overrides';

(globalThis as unknown as { __harnessOverrides: unknown }).__harnessOverrides = overrides;
startTheme();
mount(Harness, { target: document.getElementById('app')! });
