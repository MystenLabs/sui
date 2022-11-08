// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import svgr from 'vite-plugin-svgr';

import tsconfig from './tsconfig.json';

const alias = (folder: string) => new URL(folder, import.meta.url).pathname;

const tsconfigPaths = {};
Object.entries(tsconfig.compilerOptions.paths).forEach(([key, [value]]) => {
    tsconfigPaths[key] = alias(value);
});

// Assign the Vercel Analytics ID into a vite-safe name:
process.env.VITE_VERCEL_ANALYTICS_ID = process.env.VERCEL_ANALYTICS_ID;

// https://vitejs.dev/config/
export default defineConfig({
    plugins: [react(), svgr()],
    build: {
        // Set the output directory to match what CRA uses:
        outDir: 'build',
    },
    resolve: {
        alias: {
            '~': alias('./src'),
            ...tsconfigPaths,
        },
    },
});
