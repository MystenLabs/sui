// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
    root: true,
    extends: [
        'eslint:recommended',
        'plugin:@typescript-eslint/recommended',
        'plugin:react/recommended',
        'plugin:react/jsx-runtime',
        'react-app',
        'prettier',
    ],
    rules: {
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
