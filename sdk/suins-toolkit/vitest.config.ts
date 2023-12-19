// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { defineConfig } from 'vitest/config';

export default defineConfig({
    test: {
        poolOptions: {
            threads: {
                minThreads: 1,
                maxThreads: 8,
            },
        },
        hookTimeout: 1000000,
        testTimeout: 1000000,
    },
});
