<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

<template>
  <div ref="root" class="mermaid" v-html="svg" />
</template>

<script setup lang="ts">
// Renders one ```mermaid fence.
//
// Rendering happens in the browser because mermaid measures text to lay diagrams
// out, which needs a real DOM. `mermaid` is imported dynamically so it lands in
// its own chunk and is fetched only by pages that contain a diagram -- see
// ../mermaid.ts for what that is worth.
import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import mediumZoom from "medium-zoom";
import type { Zoom } from "medium-zoom";
import { useData } from "vitepress";
import { mermaidThemeVariables } from "./mermaid-theme";

const props = defineProps<{ id: string; graph: string }>();

// VitePress's own dark-mode ref, so diagrams re-render on the theme toggle
// instead of keeping colours from the mode they were first drawn in.
const { isDark } = useData();
const svg = ref("");
const root = ref<HTMLElement | null>(null);

// Click-to-zoom, the interaction GitHub provides around its own mermaid output.
// Neither mermaid nor VitePress ships one: the fence becomes an inline <svg> with
// no handler, so a click did nothing.
//
// Bound per component instance rather than globally by route: this component
// renders its own SVG asynchronously and re-renders it on the dark-mode toggle,
// so it is the only thing that knows when a target exists. A global
// `mediumZoom(".mermaid svg")` on route change races that render.
let zoom: Zoom | null = null;

const bindZoom = async (): Promise<void> => {
  await nextTick();
  // Detach first -- `renderChart` replaces the SVG on every theme toggle, which
  // would otherwise leave the zoom attached to a node no longer in the document.
  zoom?.detach();
  const target = root.value?.querySelector("svg");
  if (!target) {
    return;
  }
  zoom = mediumZoom(target, { background: "var(--vp-c-bg)", margin: 24 });
};

const renderChart = async (): Promise<void> => {
  const mermaid = (await import("mermaid")).default;

  mermaid.initialize({
    startOnLoad: false,
    // Diagram labels in these docs contain markup such as `comply (core)`; the
    // stricter levels would escape or drop it.
    securityLevel: "loose",
    // "base" is the only built-in theme that honours themeVariables wholesale,
    // which is what lets the palette follow the site instead of mermaid's stock
    // colours. Dark mode is a different set of variables, not a different theme.
    theme: "base",
    themeVariables: mermaidThemeVariables(isDark.value),
  });

  const { svg: rendered } = await mermaid.render(
    props.id,
    decodeURIComponent(props.graph),
  );
  svg.value = rendered;
  await bindZoom();
};

onMounted(renderChart);
watch(isDark, renderChart);
onUnmounted(() => zoom?.detach());
</script>

<style scoped>
.mermaid {
  margin: 20px 0;
  overflow-x: auto;
  text-align: center;
}

/* Affordance: without it the diagram gives no sign it is clickable. */
.mermaid :deep(svg) {
  cursor: zoom-in;
}
</style>
