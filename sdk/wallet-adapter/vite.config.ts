// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tsconfig from './tsconfig.json';

// TODO: Make an internal helper for this:
const alias = {};
Object.entries(tsconfig.compilerOptions.paths).forEach(([key, [value]]) => {
  alias[key] = new URL(value, import.meta.url).pathname + '/';
});

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  resolve: { alias },
});
