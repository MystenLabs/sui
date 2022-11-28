// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

<<<<<<< HEAD
import { pathAlias } from '@mysten/core/vite.config';
=======
/// <reference types="vitest" />
>>>>>>> 2f97147d9 (init)
import react from '@vitejs/plugin-react';
import svgr from 'vite-plugin-svgr';
import { defineConfig } from 'vitest/config';

// Assign the Vercel Analytics ID into a vite-safe name:
process.env.VITE_VERCEL_ANALYTICS_ID = process.env.VERCEL_ANALYTICS_ID;

// https://vitejs.dev/config/
export default defineConfig({
    plugins: [react(), svgr()] as any,
    test: {
        globals: true,
        environment: 'happy-dom',
    },
    build: {
        // Set the output directory to match what CRA uses:
        outDir: 'build',
    },
    
    resolve: {
        alias: {
            '~': new URL('./src', import.meta.url).pathname,
            ...pathAlias(import.meta.url),
        },
    },
});
