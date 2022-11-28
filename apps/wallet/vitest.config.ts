// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { pathAlias } from '@mysten/core/vite.config';
import { defineConfig } from 'vitest/config';

export default defineConfig({
    plugins: [],
    test: {
        // TODO: Create custom extension environment.
        environment: 'happy-dom',
        minThreads: 1,
        setupFiles: ['./testSetup.ts'],
    },
    resolve: {
        alias: {
            ...pathAlias(import.meta.url),
        },
    },
});
