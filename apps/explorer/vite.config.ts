// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// <reference types="vitest" />
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import pluginRewriteAll from 'vite-plugin-rewrite-all';
import svgr from 'vite-plugin-svgr';
import { configDefaults } from 'vitest/config';

process.env.VITE_VERCEL_ENV = process.env.VERCEL_ENV || 'development';

// https://vitejs.dev/config/
export default defineConfig({
	plugins: [react(), svgr(), pluginRewriteAll()],
	test: {
		// Omit end-to-end tests:
		exclude: [...configDefaults.exclude, 'tests/**'],
		css: true,
		globals: true,
		environment: 'happy-dom',
	},
	build: {
		// Set the output directory to match what CRA uses:
		outDir: 'build',
		sourcemap: true,
	},
	resolve: {
		conditions: ['source'],
		alias: {
			'~': new URL('./src', import.meta.url).pathname,
		},
	},
});
