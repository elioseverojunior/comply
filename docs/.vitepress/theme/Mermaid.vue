<!--
SPDX-FileCopyrightText: COMPLY contributors

SPDX-License-Identifier: MIT OR Apache-2.0
-->

<template>
  <div class="mermaid" v-html="svg" />
</template>

<script setup lang="ts">
// Renders one ```mermaid fence.
//
// Rendering happens in the browser because mermaid measures text to lay diagrams
// out, which needs a real DOM. `mermaid` is imported dynamically so it lands in
// its own chunk and is fetched only by pages that contain a diagram -- see
// ../mermaid.ts for what that is worth.
import { onMounted, ref, watch } from "vue";
import { useData } from "vitepress";
import { mermaidThemeVariables } from "./mermaid-theme";

const props = defineProps<{ id: string; graph: string }>();

// VitePress's own dark-mode ref, so diagrams re-render on the theme toggle
// instead of keeping colours from the mode they were first drawn in.
const { isDark } = useData();
const svg = ref("");

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

  const { svg: rendered } = await mermaid.render(props.id, decodeURIComponent(props.graph));
  svg.value = rendered;
};

onMounted(renderChart);
watch(isDark, renderChart);
</script>

<style scoped>
.mermaid {
  margin: 20px 0;
  overflow-x: auto;
  text-align: center;
}
</style>
