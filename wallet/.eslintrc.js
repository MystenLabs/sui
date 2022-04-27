// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
    root: true,
    extends: [
        'eslint:recommended',
        'plugin:@typescript-eslint/recommended',
        'plugin:react/recommended',
        'plugin:react/jsx-runtime',
        'react-app',
        'react-app/jest',
        'prettier',
    ],
    rules: {
        'react/jsx-no-bind': ['error'],
        'import/order': [
            'warn',
            {
                groups: [
                    ['builtin', 'external'],
                    ['internal', 'parent', 'sibling', 'index'],
                    'type',
                ],
                pathGroups: [
                    {
                        pattern: '{.,..}/**/*.?(s)css',
                        group: 'type',
                        position: 'after',
                    },
                ],
                'newlines-between': 'always',
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
            },
        ],
        'no-console': ['warn'],
    },
};
