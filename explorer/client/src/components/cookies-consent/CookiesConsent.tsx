// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';

async function loadAndEnableAnalytics() {
    if (process.env.NODE_ENV === 'production') {
        await import('./analytics');
    }
}

export function CookiesConsent() {
    useEffect(() => {
        (async () => {
            // @ts-expect-error no types
            await import('vanilla-cookieconsent');
            // @ts-expect-error initialized from vanilla-cookieconsent
            const cc = global.initCookieConsent();
            document.body.classList.add('c_darkmode');
            cc.run({
                revision: 0,
                autorun: true,
                current_lang: 'en',
                cookie_name: 'sui_io_cookie',
                gui_options: {
                    consent_modal: {
                        layout: 'box',
                        position: 'bottom right',
                        transition: 'slide',
                        swap_buttons: false,
                    },
                    settings_modal: {
                        layout: 'box',
                        transition: 'slide',
                    },
                },
                onAccept: function (cookie: {
                    level: ('necessary' | 'analytics')[];
                }) {
                    if (cookie?.level?.includes('analytics')) {
                        loadAndEnableAnalytics();
                    }
                },
                languages: {
                    en: {
                        consent_modal: {
                            title: 'We use cookies!',
                            description:
                                'Hi, this website uses essential cookies to ensure its proper operation and tracking cookies to understand how you interact with it. The latter will be set only upon approval. <a aria-label="Choose cookies" class="cc-link" href="#" data-cc="c-settings">Let me choose</a>',
                            primary_btn: {
                                text: 'Accept All',
                                role: 'accept_all',
                            },
                            secondary_btn: {
                                text: 'Reject All',
                                role: 'accept_necessary',
                            },
                        },
                        settings_modal: {
                            title: 'Cookie preferences',
                            save_settings_btn: 'Save settings',
                            accept_all_btn: 'Accept all',
                            reject_all_btn: 'Reject all',
                            blocks: [
                                {
                                    title: 'Cookie usage',
                                    description:
                                        'We use cookies to ensure the basic functionalities of the website and to enhance your online experience. For more details relative to cookies and other sensitive data, please read the full <a aria-label="Choose cookies" class="cc-link" href="/legal?content=privacy" target="_blank">privacy policy</a>.',
                                },
                                {
                                    title: 'Strictly necessary cookies',
                                    description:
                                        'These cookies are essential for the proper functioning of my website. Without these cookies, the website would not work properly.',
                                    toggle: {
                                        value: 'necessary',
                                        enabled: true,
                                        readonly: true,
                                    },
                                },
                                {
                                    title: 'Analytics cookies',
                                    description:
                                        'These cookies collect information about how you use the website, which pages you visited and which links you clicked on. All of the data is anonymized and cannot be used to identify you.',
                                    toggle: {
                                        value: 'analytics',
                                        enabled: false,
                                        readonly: false,
                                    },
                                },
                            ],
                        },
                    },
                },
            });
        })();
    }, []);
    return null;
}
