// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createPreset } from 'fumadocs-ui/tailwind-plugin';

/** @type {import('tailwindcss').Config} */
export default {
	content: [
		'./components/**/*.{ts,tsx}',
		'./app/**/*.{ts,tsx}',
		'./content/**/*.{md,mdx}',
		'./mdx-components.{ts,tsx}',
		'./node_modules/fumadocs-ui/dist/**/*.js',
	],
	presets: [createPreset()],
};
