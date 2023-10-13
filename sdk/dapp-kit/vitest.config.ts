// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { vanillaExtractPlugin } from '@vanilla-extract/vite-plugin';
import tsconfigPaths from 'vite-tsconfig-paths';
import { configDefaults, defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [tsconfigPaths(), vanillaExtractPlugin()],
	test: {
		exclude: [...configDefaults.exclude, 'tests/**'],
		environment: 'happy-dom',
		restoreMocks: true,
		globals: true,
		setupFiles: ['./test/setup.ts'],
	},
	resolve: {
		conditions: ['source'],
	},
});
