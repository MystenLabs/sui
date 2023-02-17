// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
  root: true,
  extends: ['react-app', 'prettier', 'plugin:prettier/recommended'],
  rules: {
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
  },
  overrides: [
    {
      files: ['*.test.*', '*.spec.*'],
      rules: {
        // Reset to defaults to allow `Buffer` usage in tests (given they run in Node and do not impact bundle):
        'no-restricted-globals': ['off'],
        '@typescript-eslint/ban-types': ['error'],
      },
    },
  ],
  settings: {
    react: {
      version: '18',
    },
  },
};
