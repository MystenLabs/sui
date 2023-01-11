// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { pathAlias } from '@mysten/core/vite.config';
import { defineConfig, configDefaults } from 'vitest/config';

export default defineConfig({
    plugins: [],
    test: {
        exclude: [...configDefaults.exclude, 'tests/**'],
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
