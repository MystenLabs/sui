// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { defineConfig } from 'vitest/config';

export default defineConfig({
    test: {
        minThreads: 1,
        setupFiles: ['./test_setup.ts'],
    },
    resolve: {
        alias: {
            '@mysten/sui.js': new URL(
                '../../sdk/typescript/src',
                import.meta.url
            ).toString(),

            '@mysten/bcs': new URL(
                '../../sdk/bcs/src',
                import.meta.url
            ).toString(),
        },
    },
});
