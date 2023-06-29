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
		'apps/icons/src',
		'next-env.d.ts',
		'doc/book',
		'external-crates',
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
			files: ['sdk/typescript/src/**/*'],
			rules: {
				'require-extensions/require-extensions': 'error',
				'require-extensions/require-index': 'error',
				'@typescript-eslint/consistent-type-imports': ['error'],
				'import/consistent-type-specifier-style': ['error', 'prefer-top-level'],
				'import/no-cycle': ['error'],
			},
		},
		{
			files: ['apps/explorer/**/*'],
			rules: {
				'import/order': [
					'warn',
					{
						groups: [['builtin', 'external'], ['internal', 'parent', 'sibling', 'index'], 'type'],
						pathGroups: [
							{
								pattern: '{.,..}/**/*.css',
								group: 'type',
								position: 'after',
							},
							{
								pattern: '~/**',
								group: 'internal',
							},
						],
						'newlines-between': 'always',
						alphabetize: { order: 'asc' },
						warnOnUnassignedImports: true,
					},
				],
				'import/no-duplicates': ['error'],
				'import/no-anonymous-default-export': 'off',
				'@typescript-eslint/consistent-type-imports': [
					'error',
					{
						prefer: 'type-imports',
						disallowTypeAnnotations: true,
						fixStyle: 'inline-type-imports',
					},
				],
				'@typescript-eslint/unified-signatures': 'error',
				'@typescript-eslint/parameter-properties': 'error',
				'react/jsx-key': ['error', {}],

				'react/boolean-prop-naming': 'off',
				'react/jsx-boolean-value': ['error', 'never'],

				// Always use function declarations for components
				'react/function-component-definition': [
					'error',
					{
						namedComponents: 'function-declaration',
						unnamedComponents: 'arrow-function',
					},
				],
				'react/prefer-stateless-function': 'error',
				'react/jsx-pascal-case': ['error', { allowAllCaps: true, allowNamespace: true }],

				// Always self-close when applicable
				'react/self-closing-comp': [
					'error',
					{
						component: true,
						html: true,
					},
				],
				'react/void-dom-elements-no-children': 'error',

				// Use alternatives instead of danger
				'react/no-danger': 'error',
				'react/no-danger-with-children': 'error',

				// Accessibility requirements
				'react/button-has-type': 'error',
				'react/no-invalid-html-attribute': 'error',

				// Security requirements
				'react/jsx-no-script-url': 'error',
				'react/jsx-no-target-blank': 'error',

				// Enforce consistent JSX spacing and syntax
				'react/jsx-no-comment-textnodes': 'error',
				'react/jsx-no-duplicate-props': 'error',
				'react/jsx-no-undef': 'error',
				'react/jsx-space-before-closing': 'off',

				// Avoid interpolation as much as possible
				'react/jsx-curly-brace-presence': ['error', { props: 'never', children: 'never' }],

				// Always use shorthand fragments when applicable
				'react/jsx-fragments': ['error', 'syntax'],
				'react/jsx-no-useless-fragment': ['error', { allowExpressions: true }],
				'react/jsx-handler-names': [
					'error',
					{
						eventHandlerPropPrefix: 'on',
					},
				],

				// Avoid bad or problematic patterns
				'react/jsx-uses-vars': 'error',
				'react/no-access-state-in-setstate': 'error',
				'react/no-arrow-function-lifecycle': 'error',
				'react/no-children-prop': 'error',
				'react/no-did-mount-set-state': 'error',
				'react/no-did-update-set-state': 'error',
				'react/no-direct-mutation-state': 'error',
				'react/no-namespace': 'error',
				'react/no-redundant-should-component-update': 'error',
				'react/no-render-return-value': 'error',
				'react/no-string-refs': 'error',
				'react/no-this-in-sfc': 'error',
				'react/no-typos': 'error',
				'react/no-unescaped-entities': 'error',
				'react/no-unknown-property': 'error',
				'react/no-unused-class-component-methods': 'error',
				'react/no-will-update-set-state': 'error',
				'react/require-optimization': 'off',
				'react/style-prop-object': 'error',
				'react/no-unstable-nested-components': 'error',

				// We may eventually want to turn this on but it requires migration:
				'react/no-array-index-key': 'off',

				// Require usage of the custom Link component:
				'no-restricted-imports': [
					'error',
					{
						paths: [
							{
								name: 'react-router-dom',
								importNames: ['Link', 'useNavigate', 'useSearchParams'],
								message:
									'Please use `LinkWithQuery`, `useSearchParamsMerged`, and `useNavigateWithQuery` from "~/ui/utils/LinkWithQuery" instead.',
							},
						],
					},
				],
				'arrow-body-style': ['error', 'as-needed'],
			},
		},
		{
			files: ['apps/wallet/**/*'],
			rules: {
				'react/display-name': 'off',
				'import/order': [
					'warn',
					{
						groups: [['builtin', 'external'], ['internal', 'parent', 'sibling', 'index'], 'type'],
						pathGroups: [
							{
								pattern: '{.,..,_*,*}/**/*.?(s)css',
								group: 'type',
								position: 'after',
							},
							{
								pattern: '_*',
								group: 'internal',
							},
							{
								pattern: '_*/**',
								group: 'internal',
							},
						],
						pathGroupsExcludedImportTypes: ['builtin', 'object', 'type'],
						alphabetize: { order: 'asc' },
						warnOnUnassignedImports: true,
					},
				],
				'import/no-duplicates': ['error'],
				'@typescript-eslint/consistent-type-imports': [
					'error',
					{
						prefer: 'type-imports',
						disallowTypeAnnotations: true,
						fixStyle: 'inline-type-imports',
					},
				],
				'@typescript-eslint/unified-signatures': 'error',
				'@typescript-eslint/parameter-properties': 'error',
				'no-console': ['warn'],
				'@typescript-eslint/no-non-null-assertion': 'off',
			},
		},
		{
			files: ['apps/wallet/src/**/*.test.*', 'apps/wallet/src/**/*.spec.*'],

			rules: {
				// Allow any casting in tests:
				'@typescript-eslint/no-explicit-any': 'off',
			},
		},
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
			files: ['sdk/ledgerjs-hw-app-sui/**/*', 'apps/wallet/**/*'],
			rules: {
				// ledgerjs-hw-app-sui and wallet use Buffer
				'no-restricted-globals': ['off'],
				'@typescript-eslint/ban-types': ['off'],
			},
		},
		{
			files: ['*.test.*', '*.spec.*'],
			rules: {
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
