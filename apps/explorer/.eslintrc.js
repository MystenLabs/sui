// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
    plugins: ['header'],
    extends: ['react-app', 'prettier'],
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
                        pattern: '{.,..}/**/*.css',
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
        'react/jsx-key': ['error', {}],
        'header/header': [
            2,
            'line',
            [
                ' Copyright (c) Mysten Labs, Inc.',
                ' SPDX-License-Identifier: Apache-2.0',
            ],
        ],
    },
};
