const defaultTheme = require('tailwindcss/defaultTheme');

module.exports = {
    content: ['./src/**/*.{js,jsx,ts,tsx}'],
    theme: {
        fontFamily: {
            sans: ['Zen Kurenaido', ...defaultTheme.fontFamily.sans],
            advanced: ['Zen Dots', 'cursive'],
            mono: ['Ubuntu Mono', ...defaultTheme.fontFamily.mono],
        },
    },
    plugins: [],
};
