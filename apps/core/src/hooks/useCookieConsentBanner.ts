// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { PersistableStorage } from '../utils/persistableStorage';
import { useProductAnalyticsConfig } from './useProductAnalyticsConfig';

export const ANALYTICS_COOKIE_CATEGORY = 'analytics';
export const NECESSARY_COOKIE_CATEGORY = 'necessary';

type CookieConsentConfig = UserConfig & { onBeforeLoad: () => void };

export function useCookieConsentBanner<T>(
	storageInstance: PersistableStorage<T>,
	options: CookieConsentConfig,
) {
	const { data: productAnalyticsConfig } = useProductAnalyticsConfig();

	useEffect(() => {
		if (productAnalyticsConfig) {
			if (productAnalyticsConfig.mustProvideCookieConsent) {
				loadCookieConsentBanner(storageInstance, options);
			} else {
				// Use cookie storage if the user doesn't have to provide consent
				storageInstance.persist();
			}
		}
	}, [options, productAnalyticsConfig, storageInstance]);
}

async function loadCookieConsentBanner<T>(
	storageInstance: PersistableStorage<T>,
	options: CookieConsentConfig,
) {
	await import('vanilla-cookieconsent');
	await import('vanilla-cookieconsent/dist/cookieconsent.css');
	await options.onBeforeLoad();

	const cookieConsent = window.initCookieConsent();
	cookieConsent.run({
		revision: 0,
		autorun: true,
		current_lang: 'en',
		gui_options: {
			consent_modal: {
				layout: 'box',
				position: 'bottom right',
				transition: 'slide',
				swap_buttons: true,
			},
			settings_modal: {
				layout: 'box',
				transition: 'slide',
			},
		},
		languages: {
			en: {
				consent_modal: {
					title: 'We use cookies!',
					description:
						'Hi, this website uses essential cookies to ensure its proper operation and tracking cookies to understand how you interact with it. The latter will be set only upon approval.',
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
								'We use cookies to ensure the basic functionalities of the website and to enhance your online experience. For more details relative to cookies and other sensitive data, please read the full <a aria-label="Choose cookies" class="cc-link" href="https://mystenlabs.com/legal?content=privacy" target="_blank">privacy policy</a>.',
						},
						{
							title: 'Strictly necessary cookies',
							description:
								'These cookies are essential for the proper functioning of my website. Without these cookies, the website would not work properly.',
							toggle: {
								value: NECESSARY_COOKIE_CATEGORY,
								enabled: true,
								readonly: true,
							},
						},
						{
							title: 'Analytics cookies',
							description:
								'These cookies collect information about how you use the website, which pages you visited and which links you clicked on. All of the data is anonymized and cannot be used to identify you.',
							toggle: {
								value: ANALYTICS_COOKIE_CATEGORY,
								enabled: false,
								readonly: false,
							},
						},
					],
				},
			},
		},
		onChange: (cookieContent) => {
			if (cookieContent.categories.includes(ANALYTICS_COOKIE_CATEGORY)) {
				storageInstance.persist();
			} else {
				storageInstance.reset();
			}
		},
		onAccept: (cookieContent) => {
			if (cookieContent.categories.includes(ANALYTICS_COOKIE_CATEGORY)) {
				storageInstance.persist();
			}
		},
		...options,
	});
}
