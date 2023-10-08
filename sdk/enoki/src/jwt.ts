// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { JWTPayload } from 'jose';
import { createRemoteJWKSet, jwtVerify } from 'jose';

const ISS_TO_JWK_SET = {
	'https://accounts.google.com': createRemoteJWKSet(
		new URL('https://www.googleapis.com/oauth2/v3/certs'),
	),
	'https://id.twitch.tv/oauth2': createRemoteJWKSet(new URL('https://id.twitch.tv/oauth2/keys')),
	'https://www.facebook.com': createRemoteJWKSet(
		new URL('https://www.facebook.com/.well-known/oauth/openid/jwks/'),
	),
} as Record<string, ReturnType<typeof createRemoteJWKSet>>;

// TODO: Call an Enoki API to do this for us, instead of doing the JWK fetching ourselves:
export async function validateJWT(jwt: string, decoded: JWTPayload) {
	if (!decoded.iss || !(decoded.iss in ISS_TO_JWK_SET)) {
		throw new Error('Invalid JWT');
	}

	await jwtVerify(jwt, ISS_TO_JWK_SET[decoded.iss], {
		// NOTE: We set clock tolerance to infinity so that we don't check JWT expiration
		clockTolerance: Infinity,
	});
}
