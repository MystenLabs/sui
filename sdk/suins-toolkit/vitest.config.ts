// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { defineConfig } from 'vitest/config';

export default defineConfig({
    test: {
        minThreads: 1,
        maxThreads: 8,
        hookTimeout: 1000000,
        testTimeout: 1000000,
    },
    resolve: {
        alias: {
            '@mysten/bcs': new URL('../bcs/src', import.meta.url).toString(),
            '@mysten/sui.js': new URL('../typescript/src', import.meta.url).toString(),
        },
    },
});
