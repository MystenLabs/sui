import { fromBase64 } from '@mysten/sui/utils';
import { beforeAll, describe, expect, it } from 'vitest';

import { AWSKMSSigner } from '../../src/index';

console.log(typeof fromBase64);

describe('AWSKMSSigner E2E Tests', () => {
	let signer: AWSKMSSigner;
	beforeAll(async () => {
		const { AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION, AWS_KMS_KEY_ID } = process.env;

		if (!AWS_ACCESS_KEY_ID || !AWS_SECRET_ACCESS_KEY || !AWS_REGION || !AWS_KMS_KEY_ID) {
			throw new Error('Missing one or more required environment variables.');
		}

		signer = new AWSKMSSigner({
			accessKeyId: AWS_ACCESS_KEY_ID,
			secretAccessKey: AWS_SECRET_ACCESS_KEY,
			region: AWS_REGION,
			kmsKeyId: AWS_KMS_KEY_ID,
		});

		await signer.prepare();
	});

	it('should retrieve the correct sui address', async () => {
		// Get the public key
		const publicKey = signer.getPublicKey();
		expect(publicKey.toSuiAddress()).toEqual(
			'0x2bfc782b6bf66f305fdeb19a203386efee3e62bce3ceb9d3d53eafbe0b14a035',
		);
	});
	it('should sign a message and verify against pubkey', async () => {
		// Define a test message
		const testMessage = 'Hello, AWS KMS Signer!';
		const messageBytes = new TextEncoder().encode(testMessage);

		// Sign the test message
		const { bytes, signature } = await signer.signPersonalMessage(messageBytes);

		// verify signature against pubkey
		const publicKey = signer.getPublicKey();
		const isValid = await publicKey.verifyPersonalMessage(messageBytes, signature);
		expect(isValid).toBe(true);
	});
});
