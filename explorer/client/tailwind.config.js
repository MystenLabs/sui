// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
            sui: {
                dark: '#1F6493',
                DEFAULT: '#6fbcf0',
                light: '#F4FBFF',
                grey: {
                    100: '#182435',
                    95: '#2A3645',
                    90: '#3D444D',
                    85: '#4E555D',
                    80: '#636870',
                    75: '#767A81',
                    70: '#898D93',
                    65: '#9C9FA4',
                    60: '#C3C5C8',
                    55: '#D7D8DA',
                    50: '#E9EAEB',
                    45: '#F0F1F2',
                    40: '#F7F8F8',
                    35: '#FEFEFE',
                },
            },
            cardDark: '#F3F4F5',
            success: '#2DD7A7',
            error: '#2DD7A7',
            header: '#2A4362',
            search: {
                fill: '#162A43',
            },
            offwhite: '#fefefe',
            offblack: '#111111',
            ...defaultColors,
        },
    },
    plugins: [],
};
