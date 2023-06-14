// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
    singleQuote: true,
    tabWidth: 4,
    overrides: [
        // tailwind plugin can be enabled for other apps in a future PR
        {
            files: 'apps/explorer/**/*',
            options: {
                plugins: ['prettier-plugin-tailwindcss'],
                tailwindConfig: './apps/explorer/tailwind.config.ts',
            },
        },
        // This should be updated to be consistent in a future PR
        {
            files: [
                'dapps/kiosk/**/*',
                'sdk/kiosk/**/*',
                'sdk/typescript/**/*',
            ],
            options: {
                printWidth: 80,
                tabWidth: 2,
                semi: true,
                trailingComma: 'all',
            },
        },
    ],
};
