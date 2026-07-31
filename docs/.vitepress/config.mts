// SPDX-FileCopyrightText: COMPLY contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

import { defineConfig } from "vitepress";
import { mermaidMarkdown, mermaidVite } from "./mermaid";

// GitHub project pages serve under `/<repo>/`, so the base has to match or every
// asset 404s. A custom domain serves from the root instead, hence the override:
// `DOCS_BASE=/ bun run build` produces a build for `comply.dev`-style hosting.
const base = process.env.DOCS_BASE ?? "/comply/";

export default defineConfig({
  base,
  title: "comply",
  description:
    "REUSE compliance in pure Rust. Checks that every file in a project declares its copyright and licence.",
  lang: "en-GB",
  cleanUrls: true,
  lastUpdated: true,

  // The architecture records were written for readers with the repository open,
  // so they link to files that exist in git but not in the site. Those links are
  // correct where they live; failing the build over them would mean rewriting
  // internal docs to suit the site rather than the other way round.
  ignoreDeadLinks: true,

  // ```mermaid fences become diagrams; see .vitepress/mermaid.ts for why the
  // plugin's `withMermaid` helper is deliberately not used.
  markdown: { config: mermaidMarkdown },
  vite: mermaidVite,

  head: [
    // `base`-prefixed by hand: entries in `head` are emitted verbatim, so a
    // bare "/favicon.svg" 404s on project pages served under /comply/.
    [
      "link",
      { rel: "icon", type: "image/svg+xml", href: `${base}favicon.svg` },
    ],
    // `alternate icon`, listed AFTER the SVG: a browser that understands
    // image/svg+xml takes the first match and ignores this, while one that does
    // not falls back here. Reversing the order would serve the 32x32 raster to
    // everyone.
    //
    // It does not silence the `GET /favicon.ico 404` in the dev console. That is
    // the browser probing the ORIGIN root, which ignores both `base` and these
    // tags -- under `/comply/` nothing can answer it.
    [
      "link",
      {
        rel: "alternate icon",
        type: "image/x-icon",
        href: `${base}favicon.ico`,
      },
    ],
    ["meta", { name: "theme-color", content: "#1f6f6b" }],
    ["meta", { property: "og:type", content: "website" }],
    [
      "meta",
      {
        property: "og:title",
        content: "comply -- REUSE compliance in pure Rust",
      },
    ],
  ],

  themeConfig: {
    nav: [
      { text: "Guide", link: "/guide/getting-started" },
      { text: "CLI", link: "/reference/cli" },
      { text: "Troubleshooting", link: "/guide/troubleshooting" },
      { text: "Parity", link: "/PARITY" },
      {
        text: "0.1.0",
        items: [
          {
            text: "Changelog",
            link: "https://github.com/elioseverojunior/comply/blob/main/CHANGELOG.md",
          },
          {
            text: "Contributing",
            link: "https://github.com/elioseverojunior/comply/blob/main/CONTRIBUTING.md",
          },
        ],
      },
    ],

    sidebar: [
      {
        text: "Guide",
        items: [
          { text: "Getting started", link: "/guide/getting-started" },
          { text: "Configuration", link: "/guide/configuration" },
          { text: "Troubleshooting", link: "/guide/troubleshooting" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "CLI", link: "/reference/cli" },
          { text: "Parity with reuse", link: "/PARITY" },
        ],
      },
      {
        text: "Internals",
        items: [
          { text: "Architecture", link: "/ARCHITECTURE" },
          { text: "Runbook", link: "/RUNBOOK" },
          { text: "Implementation plan", link: "/plan/IMPLEMENTATION" },
        ],
      },
    ],

    socialLinks: [
      { icon: "github", link: "https://github.com/elioseverojunior/comply" },
    ],

    // Bundled at build time from the page content, so search needs no external
    // service and the site stays a set of static files.
    search: { provider: "local" },

    editLink: {
      pattern:
        "https://github.com/elioseverojunior/comply/edit/main/docs/:path",
      text: "Edit this page on GitHub",
    },

    footer: {
      message:
        'Code released under <a href="https://github.com/elioseverojunior/comply/blob/main/LICENSE">MIT OR Apache-2.0</a>. Documentation under CC-BY-3.0+.',
      copyright: "Copyright (c) COMPLY contributors",
    },
  },
});
