// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module.exports = {
    presets: [require('@mysten/core/tailwind.config')],

    /*
     * NOTE: The Tailwind CSS reset doesn't mix well with the existing styles.
     * We currently disable the CSS reset and expect components to adapt accordingly.
     * When we fix this, we should use the following as a CSS reset: @tailwind base;
     */
    corePlugins: {
        preflight: false,
    },
    theme: {
        extend: {
            colors: {
                'gradient-blue-start': '#589AEA',
                'gradient-blue-end': '#4C75A6',
                'alice-blue': '#F1F8FD',
            },
            minHeight: {
                8: '2rem',
                15: '3.75rem',
            },
            spacing: {
                7.5: '1.875rem',
                8: '2rem',
                15: '3.75rem',
                'popup-height': '595px',
                'popup-width': '360px',
            },
            boxShadow: {
                'wallet-content': '0px -5px 20px 5px rgba(160, 182, 195, 0.15)',
                button: '0px 1px 2px rgba(16, 24, 40, 0.05)',
                notification: '0px 0px 20px rgba(29, 55, 87, 0.11)',
            },
            borderRadius: {
                20: '1.25rem',
                15: '0.9375rem',
                '2lg': '0.625rem',
            },
            height: {
                header: '4.25rem',
                'nav-height': '76px',
            },
            maxWidth: {
                'popup-width': '360px',
            },
            dropShadow: {
                accountModal: [
                    '0px 10px 30px rgba(0, 0, 0, 0.15)',
                    '0px 10px 50px rgba(0, 0, 0, 0.15)',
                ],
            },
        },
    },
};
