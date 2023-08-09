// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/** @type {import('tailwindcss').Config} */
const config = {
	content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
	theme: {
		extend: {
			container: {
				center: true,
				padding: '1rem',
			},
			colors: {
				primary: '#101827',
			},
		},
	},
	plugins: [require('@headlessui/tailwindcss')],
};

export default config;
