// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { configDefaults, defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    exclude: [...configDefaults.exclude, 'bcs/**'],
  },
  resolve: {
    alias: {
      '@mysten/bcs': new URL('./bcs/src', import.meta.url).toString(),
    },
  },
});
