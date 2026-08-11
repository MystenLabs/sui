// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//
// Completes the partial CodeBlock swizzle. The site swizzles
// CodeBlock/Buttons/CopyButton and CodeBlock/Buttons/WordWrapButton, which
// makes Docusaurus treat src/theme/CodeBlock/Buttons as a swizzled directory.
// Without this barrel, `@theme/CodeBlock/Buttons` (imported by theme-classic's
// CodeBlock/Layout) cannot resolve, which breaks every code block on the site.
//
// Re-exporting `@theme-original/CodeBlock/Buttons` keeps the stock Buttons
// component. It still picks up the swizzled CopyButton and WordWrapButton
// through their own `@theme` aliases.
export { default } from "@theme-original/CodeBlock/Buttons";
