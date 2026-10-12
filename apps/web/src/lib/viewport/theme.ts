// Colours for the 3D view, read from the app's CSS variables so light and dark both work.
import * as THREE from 'three';
import { isDark } from '../theme';

export interface ViewTheme {
  bg: THREE.Color;
  text: THREE.Color;
  muted: THREE.Color;
  accent: THREE.Color;
  warn: THREE.Color;
  danger: THREE.Color;
  /** CSS strings for the 2D overlay. */
  css: { text: string; muted: string; danger: string; line: string; panel: string };
}

/** The theme colour for a palette ink. */
export function inkColor(t: ViewTheme, ink: 'text' | 'muted' | 'accent' | 'warn' | 'danger' | 'station' | 'geology'): THREE.Color {
  if (ink === 'geology') return new THREE.Color(isDark() ? '#e57fd0' : '#b0138a');
  return ink === 'station' ? t.text : t[ink];
}

export function readTheme(): ViewTheme {
  const style = getComputedStyle(document.documentElement);
  const v = (name: string) => style.getPropertyValue(name).trim();
  return {
    bg: new THREE.Color(v('--panel-2')),
    text: new THREE.Color(v('--text')),
    muted: new THREE.Color(v('--muted')),
    accent: new THREE.Color(v('--accent')),
    warn: new THREE.Color(v('--warn')),
    danger: new THREE.Color(v('--danger')),
    css: { text: v('--text'), muted: v('--muted'), danger: v('--danger'), line: v('--line'), panel: v('--panel') },
  };
}

/** Low = blue, high = warm, so depth reads at a glance. `t` runs 0..1. */
export function elevationColor(t: number): THREE.Color {
  return new THREE.Color().setHSL((220 - 200 * Math.min(1, Math.max(0, t))) / 360, 0.75, 0.58);
}
