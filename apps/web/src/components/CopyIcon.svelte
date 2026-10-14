<script lang="ts">
  // The Copy command's picture: the thing with its base point, an arrow, and copies dropped further along.
  let { size = 22, strokeWidth = 1.6 }: { size?: number; strokeWidth?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width={strokeWidth} stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <rect x="2.5" y="12" width="6.5" height="6.5" rx="1" />
  <circle cx="2.5" cy="18.5" r="1.4" fill="currentColor" stroke="none" />
  <path d="M5.5 11 C 8 5, 13 4.5, 15.5 6" stroke-dasharray="1.6 2" />
  <path d="M13.8 3.9 L15.8 6.1 L13 7.3" />
  <rect x="14" y="7.5" width="6.5" height="6.5" rx="1" opacity="0.65" />
  <rect x="15.5" y="15.5" width="6.5" height="6.5" rx="1" opacity="0.35" />
</svg>
