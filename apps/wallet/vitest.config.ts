// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import tsconfigPaths from 'vite-tsconfig-paths';
import { defineConfig } from 'vitest/config';

const alias = (folder: string) => new URL(folder, import.meta.url).pathname;

export default defineConfig({
    plugins: [tsconfigPaths()],
    test: {
        // TODO: Create custom extension environment.
        environment: 'happy-dom',
        minThreads: 1,
        setupFiles: ['./testSetup.ts'],
    },
    resolve: {
        alias: {
            '@mysten/sui.js': alias('../../sdk/typescript/src'),
            '@mysten/bcs': alias('../../sdk/bcs/src'),
        },
    },
});
