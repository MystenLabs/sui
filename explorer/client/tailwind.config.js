const defaultColors = require('tailwindcss/colors');
const defaultTheme = require('tailwindcss/defaultTheme');

module.exports = {
    content: ['./src/**/*.{js,jsx,ts,tsx}'],
    theme: {
        fontFamily: {
            sans: ['Inter', ...defaultTheme.fontFamily.sans],
            advanced: ['Inter', 'cursive'],
            mono: ['Space Mono', ...defaultTheme.fontFamily.mono],
        },
        colors: {
            sui: '#6fbcf0',
            suidark: '#1670b8',
            offwhite: '#fefefe',
            offblack: '#111111',
            ...defaultColors,
        },
    },
    plugins: [],
};
