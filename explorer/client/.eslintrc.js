module.exports = {
    extends: ['react-app', 'react-app/jest', 'prettier'],
    rules: {
        'import/order': [
            'warn',
            {
                groups: [
                    ['builtin', 'external'],
                    ['internal', 'parent', 'sibling', 'index'],
                    'type',
                ],
                'newlines-between': 'always',
                alphabetize: { order: 'asc' },
            },
        ],
        '@typescript-eslint/consistent-type-imports': [
            'error',
            {
                prefer: 'type-imports',
                disallowTypeAnnotations: true,
            },
        ],
    },
};
