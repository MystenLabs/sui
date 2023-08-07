// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
    root: true,
    plugins: ['@tanstack/query'],
    extends: [
        'react-app',
        'prettier',
        'plugin:prettier/recommended',
        'plugin:@tanstack/eslint-plugin-query/recommended',
    ],
    rules: {
        'no-implicit-coercion': [
            2,
            { number: true, string: true, boolean: false },
        ],
    },
    settings: {
        react: {
            version: '18',
        },
    },
};
