// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { generateNonce } from '@mysten/zklogin';
import { randomBytes } from '@noble/hashes/utils';
import { toBigIntBE } from 'bigint-buffer';
import { base64url } from 'jose';
import Browser from 'webextension-polyfill';
import { type ZkProvider, zkProviderDataMap } from './providers';
import { fetchWithSentry } from '_src/shared/utils';

export function prepareZKLogin(currentEpoch: number) {
	const maxEpoch = currentEpoch + 2;
	const ephemeralKeyPair = new Ed25519Keypair();
	const randomness = toBigIntBE(Buffer.from(randomBytes(16)));
	const nonce = generateNonce(ephemeralKeyPair.getPublicKey(), maxEpoch, randomness);
	return {
		ephemeralKeyPair,
		randomness,
		nonce,
		maxEpoch,
	};
}

export async function zkLogin({
	provider,
	nonce,
	loginHint,
	prompt,
}: {
	provider: ZkProvider;
	nonce?: string;
	loginHint?: string;
	prompt?: 'select_account' | 'consent';
}) {
	if (!nonce) {
		nonce = base64url.encode(randomBytes(20));
	}
	const { clientID, url } = zkProviderDataMap[provider];
	const params = new URLSearchParams();
	params.append('client_id', clientID);
	params.append('response_type', 'id_token');
	params.append('redirect_uri', Browser.identity.getRedirectURL());
	params.append('scope', 'openid email profile');
	params.append('nonce', nonce);
	// This can be used for logins after the user has already connected a google account
	// and we need to make sure that the user logged in with the correct account
	if (loginHint) {
		params.append('login_hint', loginHint);
	}
	if (prompt) {
		params.append('prompt', prompt);
	}
	const authUrl = `${url}?${params.toString()}`;
	const responseURL = new URL(
		await Browser.identity.launchWebAuthFlow({
			url: authUrl,
			interactive: true,
		}),
	);
	const responseParams = new URLSearchParams(responseURL.hash.replace('#', ''));
	const jwt = responseParams.get('id_token');
	if (!jwt) {
		throw new Error('JWT is missing');
	}
	return jwt;
}

// TODO: update when we have the final production url
const pinRegistryUrl = 'https://enoki-server-7e33d356b89c.herokuapp.com';

export async function fetchPin(jwt: string): Promise<{ id: string; pin: string }> {
	const response = await fetchWithSentry('fetchUserPin', `${pinRegistryUrl}/get_pin/${jwt}`);
	return response.json();
}

type WalletInputs = {
	jwt: string;
	ephemeralPublicKey: bigint;
	maxEpoch: number;
	jwtRandomness: bigint;
	userPin: bigint;
	keyClaimName?: 'sub' | 'email';
};
type Claim = {
	name: string;
	value_base64: string;
	index_mod_4: number;
};
type ProofPoints = {
	pi_a: string[];
	pi_b: string[][];
	pi_c: string[];
};
export type PartialZkSignature = {
	proof_points: ProofPoints;
	address_seed: string;
	claims: Claim[];
	header_base64: string;
};

// TODO: update when we have the final production url (and a https one)
const zkProofsServerUrl = 'http://185.209.177.123:8000';

export async function createPartialZKSignature({
	jwt,
	ephemeralPublicKey,
	jwtRandomness,
	maxEpoch,
	userPin,
	keyClaimName = 'sub',
}: WalletInputs): Promise<PartialZkSignature> {
	const response = await fetchWithSentry('createZKProofs', `${zkProofsServerUrl}/zkp`, {
		method: 'POST',
		headers: {
			'Content-Type': 'application/json',
		},
		body: JSON.stringify({
			jwt,
			eph_public_key: ephemeralPublicKey.toString(),
			max_epoch: maxEpoch,
			jwt_randomness: jwtRandomness.toString(),
			subject_pin: userPin.toString(),
			key_claim_name: keyClaimName,
		}),
	});
	return response.json();
}
