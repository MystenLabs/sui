// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fetchWithSentry } from '_src/shared/utils';
import { type PublicKey } from '@mysten/sui.js/cryptography';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { generateNonce, generateRandomness, type ZkSignatureInputs } from '@mysten/zklogin';
import { randomBytes } from '@noble/hashes/utils';
import { toBigIntBE } from 'bigint-buffer';
import { base64url } from 'jose';
import Browser from 'webextension-polyfill';

import { zkProviderDataMap, type ZkProvider } from './providers';

export function prepareZKLogin(currentEpoch: number) {
	const maxEpoch = currentEpoch + 2;
	const ephemeralKeyPair = new Ed25519Keypair();
	const randomness = generateRandomness();
	const nonce = generateNonce(ephemeralKeyPair.getPublicKey(), maxEpoch, randomness);
	return {
		ephemeralKeyPair,
		randomness,
		nonce,
		maxEpoch,
	};
}

const forceSilentGetProviders: ZkProvider[] = ['twitch'];

/**
 * This method does a get request to the authorize url and is used as a workarround
 * for `forceSilentGetProviders` that they do the silent login/token refresh using
 * html directives or js code to redirect to the redirect_url (instead of response headers) and that forces the launchWebAuthFlow
 * to open and close quickly a new window. Which closes the popup window when open but also creates a weird flickering effect.
 *
 * @param authUrl
 */
async function tryGetRedirectURLSilently(provider: ZkProvider, authUrl: string) {
	if (!forceSilentGetProviders.includes(provider)) {
		return null;
	}
	try {
		const responseText = await (await fetch(authUrl)).text();
		const redirectURLMatch =
			/<meta\s*http-equiv="refresh"\s*(CONTENT|content)=["']0;\s?URL='(.*)'["']\s*\/?>/.exec(
				responseText,
			);
		if (redirectURLMatch) {
			const redirectURL = redirectURLMatch[2];
			if (
				redirectURL.startsWith(`https://${Browser.runtime.id}.chromiumapp.org`) &&
				redirectURL.includes('id_token=')
			) {
				return new URL(redirectURL.replaceAll('&amp;', '&'));
			}
		}
	} catch (e) {
		//do nothing
	}
	return null;
}

export async function zkLogin({
	provider,
	nonce,
	loginHint,
	prompt,
}: {
	provider: ZkProvider;
	nonce?: string;
	// This can be used for logins after the user has already connected an account
	// and we need to make sure that the user logged in with the correct account
	// seems only google supports this
	loginHint?: string;
	prompt?: boolean;
}) {
	if (!nonce) {
		nonce = base64url.encode(randomBytes(20));
	}
	const { clientID, url, extraParams, buildExtraParams } = zkProviderDataMap[provider];
	const params = new URLSearchParams(extraParams);
	params.append('client_id', clientID);
	params.append('redirect_uri', Browser.identity.getRedirectURL());
	params.append('nonce', nonce);
	if (buildExtraParams) {
		buildExtraParams({ prompt, loginHint, params });
	}
	const authUrl = `${url}?${params.toString()}`;
	let responseURL;
	if (!prompt) {
		responseURL = await tryGetRedirectURLSilently(provider, authUrl);
	}
	if (!responseURL) {
		responseURL = new URL(
			await Browser.identity.launchWebAuthFlow({
				url: authUrl,
				interactive: true,
			}),
		);
	}
	const responseParams = new URLSearchParams(responseURL.hash.replace('#', ''));
	const jwt = responseParams.get('id_token');
	if (!jwt) {
		throw new Error('JWT is missing');
	}
	return jwt;
}

const saltRegistryUrl = 'https://salt.api.mystenlabs.com';

export async function fetchSalt(jwt: string): Promise<string> {
	const response = await fetchWithSentry('fetchUserSalt', `${saltRegistryUrl}/get_salt`, {
		method: 'POST',
		headers: {
			'Content-Type': 'application/json',
		},
		body: JSON.stringify({ token: jwt }),
	});
	return (await response.json()).salt;
}

type WalletInputs = {
	jwt: string;
	ephemeralPublicKey: PublicKey;
	maxEpoch: number;
	jwtRandomness: bigint;
	userSalt: bigint;
	keyClaimName?: 'sub' | 'email';
};

export type PartialZkSignature = Omit<ZkSignatureInputs, 'addressSeed'>;

const zkProofsServerUrl = 'https://prover.mystenlabs.com/v1';

export async function createPartialZKSignature({
	jwt,
	ephemeralPublicKey,
	jwtRandomness,
	maxEpoch,
	userSalt,
	keyClaimName = 'sub',
}: WalletInputs): Promise<PartialZkSignature> {
	const response = await fetchWithSentry('createZKProofs', zkProofsServerUrl, {
		method: 'POST',
		headers: {
			'Content-Type': 'application/json',
		},
		body: JSON.stringify({
			jwt,
			extendedEphemeralPublicKey: toBigIntBE(
				Buffer.from(ephemeralPublicKey.toSuiBytes()),
			).toString(),
			maxEpoch,
			jwtRandomness: jwtRandomness.toString(),
			salt: userSalt.toString(),
			keyClaimName,
		}),
	});
	return response.json();
}
