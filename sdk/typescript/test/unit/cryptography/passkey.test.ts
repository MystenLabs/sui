// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { secp256r1 } from '@noble/curves/p256';
import { blake2b } from '@noble/hashes/blake2b';
import { sha256 } from '@noble/hashes/sha256';
import { AuthenticationCredential } from '@simplewebauthn/typescript-types';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { bcs } from '../../../src/bcs';
import { messageWithIntent } from '../../../src/cryptography';
import { PasskeyKeypair } from '../../../src/keypairs/passkey';
import {
	parseSerializedPasskeySignature,
	SECP256R1_SPKI_HEADER,
} from '../../../src/keypairs/passkey/publickey';
import { fromBase64 } from '../../../src/utils';

function compressedPubKeyToDerSPKI(compressedPubKey: Uint8Array): Uint8Array {
	// Combine header with the uncompressed public key coordinates
	const uncompressedPubKey = secp256r1.ProjectivePoint.fromHex(compressedPubKey).toRawBytes(false);
	return new Uint8Array([...SECP256R1_SPKI_HEADER, ...uncompressedPubKey]);
}

describe('passkey signer E2E testing', () => {
	beforeEach(() => {
		const sk = secp256r1.utils.randomPrivateKey();

		Object.defineProperty(global, 'navigator', {
			value: {
				credentials: {
					create: vi.fn().mockImplementation(async () => ({
						response: {
							getPublicKey: () => {
								// Existing DER-encoded SPKI public key
								return new Uint8Array([
									48, 89, 48, 19, 6, 7, 42, 134, 72, 206, 61, 2, 1, 6, 8, 42, 134, 72, 206, 61, 3,
									1, 7, 3, 66, 0, 4, 232, 238, 71, 180, 129, 19, 164, 11, 106, 184, 25, 185, 136,
									226, 178, 64, 72, 105, 218, 94, 85, 28, 244, 5, 19, 172, 167, 65, 137, 42, 193,
									31, 97, 55, 49, 168, 234, 185, 163, 251, 162, 235, 213, 185, 116, 178, 194, 7,
									128, 238, 255, 59, 121, 255, 175, 188, 137, 89, 147, 168, 103, 128, 97, 52,
								]);
							},
						},
					})),
					get: vi.fn().mockImplementation(async (options) => {
						const authenticatorData = new Uint8Array([
							88, 14, 103, 167, 58, 122, 146, 250, 216, 102, 207, 153, 185, 74, 182, 103, 89, 162,
							151, 100, 181, 113, 130, 31, 171, 174, 46, 139, 29, 123, 54, 228, 29, 0, 0, 0, 0,
						]);
						// Create clientDataJSON
						const clientDataJSON = `{"type":"webauthn.get","challenge":"${Buffer.from(options.challenge).toString('base64')}","origin":"https://www.sui.io","crossOrigin":false}`;
						// Sign authenticatorData || sha256(clientDataJSON)
						const dataToSign = new Uint8Array([...authenticatorData, ...sha256(clientDataJSON)]);
						const signature = secp256r1.sign(sha256(dataToSign), sk);
						return {
							response: {
								clientDataJSON,
								authenticatorData: authenticatorData,
								signature: signature.toDERRawBytes(),
								userHandle: null,
							},
						};
					}),
				},
			},
		});
	});

	afterEach(() => {
		vi.clearAllMocks();
	});

	it('should retrieve the correct sui address', async () => {
		const signer = await PasskeyKeypair.getPasskeyInstance();
		const publicKey = signer.getPublicKey();
		expect(publicKey.toSuiAddress()).toEqual(
			'0x05d52348e3e3a785e1e458ebe74d71e21dd4db2ba3088484cab22eca5a07da02',
		);
	});

	it('should sign a personal message and verify against pubkey', async () => {
		const sk = secp256r1.utils.randomPrivateKey();
		const pk = secp256r1.getPublicKey(sk);
		vi.mocked(navigator.credentials.create).mockImplementationOnce(
			async () =>
				({
					id: 'test',
					type: 'public-key' as PublicKeyCredentialType,
					response: {
						getPublicKey: () => {
							return compressedPubKeyToDerSPKI(pk);
						},
					},
					getClientExtensionResults: () => ({}),
				}) as unknown as Credential,
		);
		const signer = await PasskeyKeypair.getPasskeyInstance();

		const testMessage = new TextEncoder().encode('Hello world!');
		const intentMessage = messageWithIntent(
			'PersonalMessage',
			bcs.vector(bcs.U8).serialize(testMessage).toBytes(),
		);
		const digest = blake2b(intentMessage, { dkLen: 32 });

		const clientDataJSON = {
			type: 'webauthn.get',
			challenge: Buffer.from(digest).toString('base64'),
			origin: 'https://www.sui.io',
			crossOrigin: false,
		};

		// Sign authenticatorData || sha256(clientDataJSON)
		const authenticatorData = new Uint8Array([
			88, 14, 103, 167, 58, 122, 146, 250, 216, 102, 207, 153, 185, 74, 182, 103, 89, 162, 151, 100,
			181, 113, 130, 31, 171, 174, 46, 139, 29, 123, 54, 228, 29, 0, 0, 0, 0,
		]);
		const clientDataJSONString = JSON.stringify(clientDataJSON);
		const dataToSign = new Uint8Array([...authenticatorData, ...sha256(clientDataJSONString)]);
		const sigResponse = secp256r1.sign(sha256(dataToSign), sk);
		vi.mocked(navigator.credentials.get).mockImplementationOnce(async () => {
			return {
				id: 'test',
				type: 'public-key',
				authenticatorAttachment: 'platform' as const,
				rawId: new Uint8Array([1, 2, 3, 4]),
				response: {
					clientDataJSON: new TextEncoder().encode(clientDataJSONString).buffer,
					authenticatorData: authenticatorData,
					signature: sigResponse.toDERRawBytes(),
					userHandle: null,
				},
				getClientExtensionResults: () => ({}),
			} as AuthenticationCredential;
		});

		// Sign the test message
		const { signature } = await signer.signPersonalMessage(testMessage);

		// verify signature against pubkey
		const publicKey = signer.getPublicKey();
		const isValid = await publicKey.verifyPersonalMessage(testMessage, signature);
		expect(isValid).toBe(true);

		// parsed signature as expected
		const parsed = parseSerializedPasskeySignature(signature);
		expect(parsed.signatureScheme).toEqual('Passkey');
		expect(parsed.publicKey!).toEqual(pk);
		expect(new Uint8Array(parsed.authenticatorData!)).toEqual(authenticatorData);
		expect(parsed.clientDataJson).toEqual(clientDataJSONString);
	});

	it('should sign a transaction and verify against pubkey', async () => {
		const sk = secp256r1.utils.randomPrivateKey();
		const pk = secp256r1.getPublicKey(sk);
		vi.mocked(navigator.credentials.create).mockImplementationOnce(
			async () =>
				({
					id: 'test',
					type: 'public-key' as PublicKeyCredentialType,
					response: {
						getPublicKey: () => {
							return compressedPubKeyToDerSPKI(pk);
						},
					},
					getClientExtensionResults: () => ({}),
				}) as unknown as Credential,
		);
		const signer = await PasskeyKeypair.getPasskeyInstance();
		const messageBytes = fromBase64(
			'AAABACACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgEBAQABAABnEUWt6SNz7OPa4hXLyCw9tI5Y7rNxhh5DFljH1jLT6QEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAIMqiyOLCIblSqii0TkS8PjMoj3tmA7S24hBMyonz2Op/ZxFFrekjc+zj2uIVy8gsPbSOWO6zcYYeQxZYx9Yy0+noAwAAAAAAAICWmAAAAAAAAA==',
		);
		const intentMessage = messageWithIntent('TransactionData', messageBytes);
		const digest = blake2b(intentMessage, { dkLen: 32 });
		const authenticatorData = new Uint8Array([
			88, 14, 103, 167, 58, 122, 146, 250, 216, 102, 207, 153, 185, 74, 182, 103, 89, 162, 151, 100,
			181, 113, 130, 31, 171, 174, 46, 139, 29, 123, 54, 228, 29, 0, 0, 0, 0,
		]);
		// Create clientDataJSON
		const clientDataJSON = {
			type: 'webauthn.get',
			challenge: Buffer.from(digest).toString('base64'),
			origin: 'https://www.sui.io',
			crossOrigin: false,
		};
		const clientDataJSONString = JSON.stringify(clientDataJSON);

		// Sign authenticatorData || sha256(clientDataJSON)
		const dataToSign = new Uint8Array([...authenticatorData, ...sha256(clientDataJSONString)]);
		const sigResponse = secp256r1.sign(sha256(dataToSign), sk);

		vi.mocked(navigator.credentials.get).mockImplementationOnce(async () => {
			return {
				id: 'test',
				type: 'public-key',
				authenticatorAttachment: 'platform' as const,
				rawId: new Uint8Array([1, 2, 3, 4]),
				response: {
					clientDataJSON: new TextEncoder().encode(clientDataJSONString).buffer,
					authenticatorData: authenticatorData,
					signature: sigResponse.toDERRawBytes(),
					userHandle: null,
				},
				getClientExtensionResults: () => ({}),
			} as AuthenticationCredential;
		});

		// Sign the test message
		const { signature } = await signer.signTransaction(messageBytes);

		// verify signature against pubkey
		const publicKey = signer.getPublicKey();
		const isValid = await publicKey.verifyTransaction(messageBytes, signature);
		expect(isValid).toBe(true);

		// parsed signature as expected
		const parsed = parseSerializedPasskeySignature(signature);
		expect(parsed.signatureScheme).toEqual('Passkey');
		expect(parsed.publicKey!).toEqual(pk);
		expect(new Uint8Array(parsed.authenticatorData!)).toEqual(authenticatorData);
		expect(parsed.clientDataJson).toEqual(clientDataJSONString);
	});
});
