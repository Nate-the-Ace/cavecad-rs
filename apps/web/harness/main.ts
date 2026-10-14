import { mount } from 'svelte';
import '../src/app.css';
import { startTheme } from '../src/lib/theme';
import { cave } from '../src/lib/cave.svelte';
import Harness from './Harness.svelte';

startTheme();
cave.trip = 't1';
mount(Harness, { target: document.getElementById('app')! });
