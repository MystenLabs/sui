// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
    root: true,
    extends: ['react-app', 'prettier', 'plugin:prettier/recommended'],
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
