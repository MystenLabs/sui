// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { beforeAll, describe, expect, it } from 'vitest';

import { GcpKmsSigner } from '../src/gcp/gcp-kms-client';

const { E2E_GCP_KMS_TEST_ENABLE } = process.env;

describe.runIf(E2E_GCP_KMS_TEST_ENABLE)('GCP KMS signer E2E testing', () => {
	let signer: GcpKmsSigner;
	beforeAll(async () => {
		const {
			GOOGLE_PROJECT_ID,
			GOOGLE_LOCATION,
			GOOGLE_KEYRING,
			GOOGLE_KEY_NAME,
			GOOGLE_KEY_NAME_VERSION,
		} = process.env;

		if (
			!GOOGLE_PROJECT_ID ||
			!GOOGLE_LOCATION ||
			!GOOGLE_KEYRING ||
			!GOOGLE_KEY_NAME ||
			!GOOGLE_KEY_NAME_VERSION
		) {
			throw new Error('Missing one or more required environment variables.');
		}

		signer = await GcpKmsSigner.fromOptions({
			projectId: GOOGLE_PROJECT_ID,
			location: GOOGLE_LOCATION,
			keyRing: GOOGLE_KEYRING,
			cryptoKey: GOOGLE_KEY_NAME,
			cryptoKeyVersion: GOOGLE_KEY_NAME_VERSION,
		});
	});

	it('should retrieve the correct sui address', async () => {
		const publicKey = signer.getPublicKey();

		expect(publicKey.toSuiAddress()).toEqual(
			'0x2ac50bf55beac50aa004c6ac1f46a058e21c86980303d87b8e3b3d3fa7b8d9eb',
		);
	});

	it('should sign a message and verify against pubkey', async () => {
		// Define a test message
		const testMessage = 'Hello, GCP KMS Signer!';
		const messageBytes = new TextEncoder().encode(testMessage);

		// Sign the test message
		const { signature } = await signer.signPersonalMessage(messageBytes);

		// verify signature against pubkey
		const publicKey = signer.getPublicKey();
		const isValid = await publicKey.verifyPersonalMessage(messageBytes, signature);
		expect(isValid).toBe(true);
	});
});
