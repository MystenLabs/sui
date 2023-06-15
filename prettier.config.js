// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
  singleQuote: true,
  tabWidth: 2,
  trailingComma: 'all',
  semi: true,
  overrides: [
    // tailwind plugin can be enabled for other apps in a future PR
    {
      files: 'apps/explorer/**/*',
      options: {
        plugins: ['prettier-plugin-tailwindcss'],
        tailwindConfig: './apps/explorer/tailwind.config.ts',
      },
    },
    {
      files: ['apps/**/*', 'sdk/ledgerjs-hw-app-sui/**/*'],
      options: {
        tabWidth: 4,
        trailingComma: 'es5',
      },
    },
  ],
};
