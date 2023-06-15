// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const IPFS_START_STRING = 'https://ipfs.io/ipfs/';

export function hexToAscii(hex: string) {
	if (!hex || typeof hex != 'string') return;
	hex = hex.replace(/^0x/, '');

	var str = '';
	for (var n = 0; n < hex.length; n += 2)
		str += String.fromCharCode(parseInt(hex.substring(n, 2), 16));

	return str;
}

export const trimStdLibPrefix = (str: string): string => str.replace(/^0x2::/, '');

export const findIPFSvalue = (url: string): string | undefined => url.match(/^ipfs:\/\/(.*)/)?.[1];

export function transformURL(url: string) {
	const found = findIPFSvalue(url);
	if (!found) {
		return url;
	}
	return `${IPFS_START_STRING}${found}`;
}

export async function extractFileType(displayString: string, signal: AbortSignal) {
	// First check Content-Type in header:
	const result = await fetch(transformURL(displayString), {
		signal: signal,
	})
		.then((resp) => resp?.headers?.get('Content-Type')?.split('/').reverse()?.[0])
		.catch((err) => console.error(err));

	// Return the Content-Type if found:
	if (result) {
		return result;
	}
	// When Content-Type cannot be accessed (e.g. because of CORS), rely on file extension
	const extension = displayString?.split('.').reverse()?.[0] || '';
	if (['jpg', 'jpeg', 'png'].includes(extension)) {
		return extension;
	} else {
		return 'Image';
	}
}

export async function genFileTypeMsg(displayString: string, signal: AbortSignal) {
	return extractFileType(displayString, signal)
		.then((result) => (result === 'Image' ? result : result.toUpperCase()))
		.then((result) => `1 ${result} File`)
		.catch((err) => {
			console.error(err);
			return `1 Image File`;
		});
}
