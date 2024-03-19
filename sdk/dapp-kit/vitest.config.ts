// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// <reference types="vitest" />

import { vanillaExtractPlugin } from '@vanilla-extract/vite-plugin';
import { defineConfig } from 'vite';
import { configDefaults } from 'vitest/config';

export default defineConfig({
	plugins: [vanillaExtractPlugin()],
	test: {
		exclude: [...configDefaults.exclude, 'tests/**'],
		environment: 'jsdom',
		restoreMocks: true,
		globals: true,
		setupFiles: ['./test/setup.ts'],
	},
	resolve: {
		alias: {
			// TODO: Figure out a better way to run tests that avoids these aliases:
			'@mysten/wallet-standard': new URL('../wallet-standard/src', import.meta.url).pathname,
			'@mysten/bcs': new URL('../bcs/src', import.meta.url).pathname,
			'@mysten/sui.js/keypairs/ed25519': new URL(
				'../typescript/src/keypairs/ed25519',
				import.meta.url,
			).pathname,
			'@mysten/sui.js/client': new URL('../typescript/src/client', import.meta.url).pathname,
			'@mysten/sui.js/utils': new URL('../typescript/src/utils', import.meta.url).pathname,
			'@mysten/sui.js/transactions': new URL('../typescript/src/transactions', import.meta.url)
				.pathname,
		},
	},
});
