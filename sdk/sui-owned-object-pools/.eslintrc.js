// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
  plugins: [
    '@typescript-eslint',
    'unused-imports',
    'prettier',
    'header',
    'require-extensions',
    'import',
  ],
  extends: [
    'eslint:recommended',
    'prettier',
    'plugin:prettier/recommended',
    'plugin:import/typescript',
  ],
  settings: {
    'import/resolver': {
      typescript: {},
    },
  },
  parserOptions: {
    sourceType: 'module',
  },
  parser: '@typescript-eslint/parser',
  env: {
    es2020: true,
    node: true,
    jest: true,
  },
  root: true,
  ignorePatterns: ['node_modules', 'build', 'dist'],
  rules: {
    'no-case-declarations': 'off',
    'no-implicit-coercion': [2, { number: true, string: true, boolean: false }],
    '@typescript-eslint/no-redeclare': 'off',
    '@typescript-eslint/ban-types': [
      'error',
      {
        types: {
          Buffer:
            'Buffer usage increases bundle size and is not consistently implemented on web.',
        },
        extendDefaults: true,
      },
    ],
    'no-restricted-globals': [
      'error',
      {
        name: 'Buffer',
        message:
          'Buffer usage increases bundle size and is not consistently implemented on web.',
      },
    ],
    'header/header': [
      2,
      'line',
      [
        ' Copyright (c) Mysten Labs, Inc.',
        ' SPDX-License-Identifier: Apache-2.0',
      ],
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
    'require-extensions/require-extensions': 'off',
    'require-extensions/require-index': 'error',
    '@typescript-eslint/consistent-type-imports': ['error'],
    'import/no-cycle': 'error',
    'import/consistent-type-specifier-style': ['error', 'prefer-top-level'],
  },
};
