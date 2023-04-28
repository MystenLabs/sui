// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Config } from 'tailwindcss';
import { fontFamily } from 'tailwindcss/defaultTheme';
import colors from 'tailwindcss/colors';

export default {
    content: ['./src/**/*.{js,jsx,ts,tsx}'],
    theme: {
        // Overwrite colors to avoid accidental usage of Tailwind colors:
        colors: {
            white: colors.white,
            black: colors.black,
            transparent: colors.transparent,
            inherit: colors.inherit,

            gray: {
                100: '#182435',
                95: '#2A3645',
                90: '#383F47',
                85: '#5A6573',
                80: '#636870',
                75: '#767A81',
                70: '#898D93',
                65: '#9C9FA4',
                60: '#C3C5C8',
                55: '#D7D8DA',
                50: '#E9EAEB',
                45: '#E3E6E8',
                40: '#F3F6F8',
                35: '#FEFEFE',
            },

            sui: {
                DEFAULT: '#6fbcf0',
                bright: '#2A38EB',
                light: '#E1F3FF',
                lightest: '#F1F8FD',
                dark: '#1F6493',
            },

            steel: {
                DEFAULT: '#A0B6C3',
                dark: '#758F9E',
                darker: '#566873',
            },

            issue: {
                DEFAULT: '#FF794B',
                dark: '#EB5A29',
                light: '#FFECE6',
            },
            hero: {
                DEFAULT: '#0284AD',
                dark: '#007195',
                darkest: '#15527B',
            },
            success: {
                DEFAULT: '#2DD7A7',
                dark: '#008C65',
                light: '#D5F7EE',
            },
            warning: {
                DEFAULT: '#F2BD24',
                dark: '#8D6E15',
                light: '#FFF8E2',
            },
            headerNav: '#2A4362',
            search: {
                fill: '#162A43',
            },
            offwhite: '#fefefe',
            offblack: '#111111',
            ebony: '#101828',
        },

        extend: {
            colors: {
                'gradient-blue-start': '#589AEA',
                'gradient-blue-end': '#4C75A6',
            },
            // Line-heights that are found in the design:
            lineHeight: {
                80: '0.8',
                100: '1',
                130: '1.3',
            },
            boxShadow: {
                notification: '0px 0px 20px rgba(29, 55, 87, 0.11)',
                moduleOption:
                    '0px 0px 29px rgba(0, 0, 0, 0.13), 0px 0px 44px rgba(0, 0, 0, 0.15), 0px 0px 0px rgba(0, 0, 0, 0.2)',
                blurXl: '0 0 20px 0 rgba(0, 0, 0, 0.3)',
                button: '0px 1px 2px rgba(16, 24, 40, 0.05)',
                xs: '0px 1px 2px rgba(16, 24, 40, 0.05)',
                DEFAULT:
                    '0px 5px 30px rgba(86, 104, 115, 0.2), 0px 0px 0px 1px rgba(160, 182, 195, 0.08)',
            },
            fontSize: {
                // Text sizes:
                body: ['14px', '1'],
                bodySmall: ['13px', '1'],
                subtitle: ['12px', '1'],
                subtitleSmall: ['11px', '1'],
                subtitleSmallExtra: ['10px', '1'],
                caption: ['12px', '1'],
                captionSmall: ['11px', '1'],
                captionSmallExtra: ['10px', '1'],
                iconTextLarge: ['48px', '1'],

                // Heading sizes:
                heading1: ['28px', '1'],
                heading2: ['24px', '1'],
                heading3: ['20px', '1'],
                heading4: ['18px', '1'],
                heading5: ['17px', '1'],
                heading6: ['16px', '1'],

                // Paragraph sizes:
                pHeading6: ['16px', '1.4'],
                pBody: ['14px', '1.4'],
                pBodySmall: ['13px', '1.4'],
                pSubtitle: ['12px', '1.4'],
                pSubtitleSmall: ['11px', '1.4'],
            },
            fontFamily: {
                system: fontFamily.sans,
                sans: ['InterVariable', 'Inter', ...fontFamily.sans],
                mono: [
                    'Red Hat MonoVariable',
                    'Red Hat Mono',
                    ...fontFamily.mono,
                ],
            },
            keyframes: {
                shimmer: {
                    '0%': { 'background-position': '-1000px 0' },
                    '100%': { 'background-position': '1000px 0' },
                },
            },
            animation: {
                shimmer: 'shimmer 2s infinite linear',
            },
            spacing: {
                1.25: '0.3125rem',
                3.75: '0.9375rem',
                4.5: '1.125rem',
                7.5: '1.875rem',
                50: '12.5rem',
                verticalListShort: '13.0625rem',
                verticalListLong: '35.6875rem',
                600: '37.5rem',
            },
            height: {
                header: '68px',
                31.5: '7.5rem',
            },
            width: {
                31.5: '7.5rem',
            },
            transitionTimingFunction: {
                'ease-in-out-cubic': 'cubic-bezier(0.65, 0, 0.35, 1)',
                'ease-out-cubic': 'cubic-bezier(0.33, 1, 0.68, 1)',
            },
            transitionDuration: {
                400: '400ms',
            },
            backgroundImage: {
                placeholderGradient01:
                    'linear-gradient(165.96deg, #e6f5ff 10%, #ebecff 95%)',
                placeholderShimmer:
                    'linear-gradient(90deg, #ecf1f4 -24.18%, rgba(237 242 245 / 40%) 73.61%, #f3f7f9 114.81%, #ecf1f4 114.82%)',
            },
            rotate: {
                135: '135deg',
            },
            borderRadius: {
                '2lg': '0.625rem',
            },
        },
    },
    corePlugins: {
        aspectRatio: false,
    },
    plugins: [
        require('@headlessui/tailwindcss'),
        require('@tailwindcss/aspect-ratio'),
        require('@tailwindcss/forms')({
            strategy: 'class',
        }),
    ],
} satisfies Config;
