// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import react from '@vitejs/plugin-react';
import path from 'path';
import { defineConfig } from 'vite';
import svgr from 'vite-plugin-svgr';

// https://vitejs.dev/config/
export default defineConfig({
    plugins: [react(), svgr()],
    build: {
        // Set the output directory to match what CRA uses:
        outDir: 'build',
    },
    resolve: {
        alias: {
            '@mysten/sui.js': path.resolve(
                __dirname,
                '../../sdk/typescript/src/'
            ),
            '@mysten/bcs': path.resolve(
                __dirname,
                '../../sdk/typescript/bcs/src/'
            ),
        },
    },
});
