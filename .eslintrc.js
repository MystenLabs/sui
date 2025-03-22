// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
	plugins: ['@tanstack/query', 'unused-imports', 'prettier', 'header', 'require-extensions'],
	extends: [
		'eslint:recommended',
		'react-app',
		'plugin:@tanstack/eslint-plugin-query/recommended',
		'prettier',
		'plugin:prettier/recommended',
		'plugin:import/typescript',
	],
	settings: {
		react: {
			version: '18',
		},
		'import/resolver': {
			typescript: true,
		},
	},
	env: {
		es2020: true,
	},
	root: true,
	ignorePatterns: [
		'node_modules',
		'build',
		'dist',
		'coverage',
		'next-env.d.ts',
		'doc/book',
		'external-crates',
		'storybook-static',
		'.next',
	],
	rules: {
		'no-case-declarations': 'off',
		'no-implicit-coercion': [2, { number: true, string: true, boolean: false }],
		'@typescript-eslint/no-redeclare': 'off',
		'@typescript-eslint/ban-types': [
			'error',
			{
				types: {
					Buffer: 'Buffer usage increases bundle size and is not consistently implemented on web.',
				},
				extendDefaults: true,
			},
		],
		'no-restricted-globals': [
			'error',
			{
				name: 'Buffer',
				message: 'Buffer usage increases bundle size and is not consistently implemented on web.',
			},
		],
		'header/header': [
			2,
			'line',
			[' Copyright (c) Mysten Labs, Inc.', ' SPDX-License-Identifier: Apache-2.0'],
		],
		'@typescript-eslint/no-unused-vars': [
			'error',
			{
				argsIgnorePattern: '^_',
				varsIgnorePattern: '^_',
				vars: 'all',
				args: 'none',
				ignoreRestSiblings: true,
			},
		],
	},
	overrides: [
		{
			files: ['dapps/kiosk/**/*'],
			rules: {
				'no-unused-vars': 'off', // or "@typescript-eslint/no-unused-vars": "off",
				'unused-imports/no-unused-imports': 'error',
				'unused-imports/no-unused-vars': [
					'warn',
					{
						vars: 'all',
						varsIgnorePattern: '^_',
						args: 'after-used',
						argsIgnorePattern: '^_',
					},
				],
			},
		},
		{
			files: ['*.test.*', '*.spec.*'],
			rules: {
				// Tests can violate extension rules:
				'require-extensions/require-extensions': 'off',
				'require-extensions/require-index': 'off',
				'@typescript-eslint/consistent-type-imports': ['off'],
				'import/consistent-type-specifier-style': ['off'],
				// Reset to defaults to allow `Buffer` usage in tests (given they run in Node and do not impact bundle):
				'no-restricted-globals': ['off'],
				'@typescript-eslint/ban-types': ['error'],
			},
		},
		{
			files: ['*.stories.*'],
			rules: {
				// Story files have render functions that this rule incorrectly warns on:
				'react-hooks/rules-of-hooks': 'off',
			},
		},
	],
};
