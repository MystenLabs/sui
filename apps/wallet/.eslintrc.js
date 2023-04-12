// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
    root: true,
    extends: [
        'eslint:recommended',
        'plugin:@typescript-eslint/recommended',
        'react-app',
        'prettier',
    ],
    rules: {
        'no-implicit-coercion': [
            2,
            { number: true, string: true, boolean: false },
        ],
        'react/display-name': 'off',
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
                fixStyle: 'inline-type-imports',
            },
        ],
        '@typescript-eslint/unified-signatures': 'error',
        '@typescript-eslint/parameter-properties': 'error',
        'no-console': ['warn'],
        '@typescript-eslint/no-non-null-assertion': 'off',
    },
    overrides: [
        {
            files: ['*.test.*', '*.spec.*'],
            rules: {
                // Allow any casting in tests:
                '@typescript-eslint/no-explicit-any': 'off',
            },
        },
    ],
};
