module.exports = {
    extends: ['react-app', 'react-app/jest', 'prettier'],
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
    },
};
