// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
  root: true,
  extends: ['react-app', 'prettier', 'plugin:prettier/recommended'],
  rules: {
    '@typescript-eslint/ban-types': [
      'error',
      {
        types: {
          Buffer:
            'Buffer usage increases bundle size and is not consistently implemented on web.',
        },
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
  settings: {
    react: {
      version: '18',
    },
  },
};
