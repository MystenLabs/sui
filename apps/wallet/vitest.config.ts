// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import tsconfigPaths from 'vite-tsconfig-paths';
import { defineConfig, configDefaults } from 'vitest/config';

export default defineConfig({
	plugins: [tsconfigPaths()],
	test: {
		exclude: [...configDefaults.exclude, 'tests/**'],
		// TODO: Create custom extension environment.
		environment: 'happy-dom',
		minThreads: 1,
		setupFiles: ['./testSetup.ts'],
		restoreMocks: true,
	},
	resolve: {
		conditions: ['source'],
	},
});
