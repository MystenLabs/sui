// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAppSelector } from '_hooks';
import { setAttributes } from '_src/shared/experimentation/features';
import { useGrowthBook } from '@growthbook/growthbook-react';
import { fromBase64, toBase64 } from '@mysten/sui/utils';
import * as Sentry from '@sentry/browser';
import { useEffect } from 'react';
import Browser from 'webextension-polyfill';

import { getUrlWithDeviceId } from '../analytics/amplitude';

export const MAIN_UI_URL = Browser.runtime.getURL('ui.html');

const MYSTEN_LABS_DAPPS = ['suifrens.com', 'suins.io'];

export function openInNewTab() {
	return Browser.tabs.create({ url: MAIN_UI_URL });
}

export function useSetGrowthbookAttributes() {
	const { apiEnv, customRPC } = useAppSelector((state) => state.app);
	const growthBook = useGrowthBook();

	useEffect(() => {
		if (growthBook) {
			setAttributes({ apiEnv, customRPC });
		}
	}, [growthBook, apiEnv, customRPC]);
}

export function isValidUrl(url: string | null) {
	if (!url) {
		return false;
	}
	try {
		new URL(url);
		return true;
	} catch (e) {
		return false;
	}
}

export function getDAppUrl(appUrl: string) {
	const url = new URL(appUrl);
	const isMystenLabsDApp = MYSTEN_LABS_DAPPS.includes(url.hostname);
	return isMystenLabsDApp ? getUrlWithDeviceId(url) : url;
}

export function getValidDAppUrl(appUrl: string) {
	try {
		return getDAppUrl(appUrl);
	} catch (error) {
		/* empty */
	}
	return null;
}

export function prepareLinkToCompare(link: string) {
	let adjLink = link.toLowerCase();
	if (!adjLink.endsWith('/')) {
		adjLink += '/';
	}
	return adjLink;
}

/**
 * Includes ? when query string is set
 */
export function toSearchQueryString(searchParams: URLSearchParams) {
	const searchQuery = searchParams.toString();
	if (searchQuery) {
		return `?${searchQuery}`;
	}
	return '';
}

export function toUtf8OrB64(message: string | Uint8Array) {
	const messageBytes = typeof message === 'string' ? fromBase64(message) : message;
	let messageToReturn: string = typeof message === 'string' ? message : toBase64(message);
	let type: 'utf8' | 'base64' = 'base64';
	try {
		messageToReturn = new TextDecoder('utf8', { fatal: true }).decode(messageBytes);
		type = 'utf8';
	} catch (e) {
		// do nothing
	}
	return {
		message: messageToReturn,
		type,
	};
}

export async function fetchWithSentry(name: string, ...params: Parameters<typeof fetch>) {
	const url = params[0] instanceof URL ? params[0].href : String(params[0]);
	const transaction = Sentry.startTransaction({
		name,
		op: 'http.request',
		tags: {
			url,
		},
	});
	try {
		const response = await fetch(...params);
		if (!response.ok) {
			throw new Error(`Request failed with status ${response.status} (${response.statusText})`);
		}
		transaction.setStatus('ok' as Sentry.SpanStatusType);
		return response;
	} catch (e) {
		transaction.setStatus('unknown_error' as Sentry.SpanStatusType);
		throw e;
	} finally {
		transaction.finish();
	}
}
