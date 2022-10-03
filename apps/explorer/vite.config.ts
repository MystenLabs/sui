// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import svgr from 'vite-plugin-svgr';

const alias = (folder: string) => new URL(folder, import.meta.url).pathname;

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
            '@mysten/sui.js': alias('../../sdk/typescript/src/'),
            '@mysten/bcs': alias('../../sdk/bcs/src/'),
        },
    },
});
