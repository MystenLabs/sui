// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { secp256r1 } from '@noble/curves/p256';
import { blake2b } from '@noble/hashes/blake2b';
import { sha256 } from '@noble/hashes/sha256';
import { AuthenticationCredential, RegistrationCredential } from '@simplewebauthn/typescript-types';
import { describe, expect, it } from 'vitest';

import { bcs } from '../../../src/bcs';
import { messageWithIntent } from '../../../src/cryptography';
import { PasskeyKeypair } from '../../../src/keypairs/passkey';
import { PasskeyProvider } from '../../../src/keypairs/passkey/keypair';
import {
	parseSerializedPasskeySignature,
	PasskeyPublicKey,
	SECP256R1_SPKI_HEADER,
} from '../../../src/keypairs/passkey/publickey';
import { fromBase64 } from '../../../src/utils';

function compressedPubKeyToDerSPKI(compressedPubKey: Uint8Array): Uint8Array {
	// Combine header with the uncompressed public key coordinates.
	const uncompressedPubKey = secp256r1.ProjectivePoint.fromHex(compressedPubKey).toRawBytes(false);
	return new Uint8Array([...SECP256R1_SPKI_HEADER, ...uncompressedPubKey]);
}

class MockPasskeySigner implements PasskeyProvider {
	private sk: Uint8Array;
	private authenticatorData: Uint8Array;
	private pk: Uint8Array | null;
	private changeDigest: boolean;
	private changeClientDataJson: boolean;
	private changeAuthenticatorData: boolean;
	private changeSignature: boolean;

	constructor(options?: {
		sk?: Uint8Array;
		pk?: Uint8Array;
		authenticatorData?: Uint8Array;
		changeDigest?: boolean;
		changeClientDataJson?: boolean;
		changeAuthenticatorData?: boolean;
		changeSignature?: boolean;
	}) {
		this.sk = options?.sk ?? secp256r1.utils.randomPrivateKey();
		this.pk = options?.pk ?? null;
		this.authenticatorData =
			options?.authenticatorData ??
			new Uint8Array([
				88, 14, 103, 167, 58, 122, 146, 250, 216, 102, 207, 153, 185, 74, 182, 103, 89, 162, 151,
				100, 181, 113, 130, 31, 171, 174, 46, 139, 29, 123, 54, 228, 29, 0, 0, 0, 0,
			]);
		this.changeDigest = options?.changeDigest ?? false;
		this.changeClientDataJson = options?.changeClientDataJson ?? false;
		this.changeAuthenticatorData = options?.changeAuthenticatorData ?? false;
		this.changeSignature = options?.changeSignature ?? false;
	}

	async create(): Promise<RegistrationCredential> {
		const pk = this.pk;
		const credentialResponse: AuthenticatorAttestationResponse = {
			attestationObject: new Uint8Array(),
			clientDataJSON: new TextEncoder().encode(
				JSON.stringify({
					type: 'webauthn.create',
					challenge: '',
					origin: 'https://www.sui.io',
					crossOrigin: false,
				}),
			),
			getPublicKey: () =>
				pk
					? compressedPubKeyToDerSPKI(pk)
					: new Uint8Array([
							48, 89, 48, 19, 6, 7, 42, 134, 72, 206, 61, 2, 1, 6, 8, 42, 134, 72, 206, 61, 3, 1, 7,
							3, 66, 0, 4, 232, 238, 71, 180, 129, 19, 164, 11, 106, 184, 25, 185, 136, 226, 178,
							64, 72, 105, 218, 94, 85, 28, 244, 5, 19, 172, 167, 65, 137, 42, 193, 31, 97, 55, 49,
							168, 234, 185, 163, 251, 162, 235, 213, 185, 116, 178, 194, 7, 128, 238, 255, 59, 121,
							255, 175, 188, 137, 89, 147, 168, 103, 128, 97, 52,
						]),
			getPublicKeyAlgorithm: () => -7,
			getTransports: () => ['usb', 'ble', 'nfc'],
			getAuthenticatorData: () => this.authenticatorData,
		};

		const credential: PublicKeyCredential = {
			id: 'mock-credential-id',
			rawId: new Uint8Array([1, 2, 3]),
			response: credentialResponse,
			type: 'public-key',
			authenticatorAttachment: 'cross-platform',
			getClientExtensionResults: () => ({}),
		};

		return credential as RegistrationCredential;
	}

	async get(challenge: Uint8Array): Promise<AuthenticationCredential> {
		// Manually mangle the digest bytes if changeDigest.
		if (this.changeDigest) {
			challenge = sha256(challenge);
		}

		const clientDataJSON = this.changeClientDataJson
			? JSON.stringify({
					type: 'webauthn.create', // Wrong type for clientDataJson.
					challenge: Buffer.from(challenge).toString('base64'),
					origin: 'https://www.sui.io',
					crossOrigin: false,
				})
			: JSON.stringify({
					type: 'webauthn.get',
					challenge: Buffer.from(challenge).toString('base64'),
					origin: 'https://www.sui.io',
					crossOrigin: false,
				});

		// Sign authenticatorData || sha256(clientDataJSON).
		const dataToSign = new Uint8Array([
			...this.authenticatorData,
			...sha256(new TextEncoder().encode(clientDataJSON)),
		]);

		// Manually mangle the signature if changeSignature.
		const signature = this.changeSignature
			? secp256r1.sign(sha256(dataToSign), secp256r1.utils.randomPrivateKey())
			: secp256r1.sign(sha256(dataToSign), this.sk);

		const authResponse: AuthenticatorAssertionResponse = {
			authenticatorData: this.changeAuthenticatorData
				? new Uint8Array([1]) // Change authenticator data
				: this.authenticatorData,
			clientDataJSON: new TextEncoder().encode(clientDataJSON),
			signature: signature.toDERRawBytes(),
			userHandle: null,
		};

		const credential: PublicKeyCredential = {
			id: 'mock-credential-id',
			rawId: new Uint8Array([1, 2, 3]),
			type: 'public-key',
			response: authResponse,
			authenticatorAttachment: 'cross-platform',
			getClientExtensionResults: () => ({}),
		};

		return credential as AuthenticationCredential;
	}
}

describe('passkey signer E2E testing', () => {
	it('should retrieve the correct sui address', async () => {
		const mockProvider = new MockPasskeySigner();
		const signer = await PasskeyKeypair.getPasskeyInstance(mockProvider);
		const publicKey = signer.getPublicKey();
		expect(publicKey.toSuiAddress()).toEqual(
			'0x05d52348e3e3a785e1e458ebe74d71e21dd4db2ba3088484cab22eca5a07da02',
		);
	});

	it('should sign a personal message and verify against pubkey', async () => {
		const sk = secp256r1.utils.randomPrivateKey();
		const pk = secp256r1.getPublicKey(sk);
		const authenticatorData = new Uint8Array([
			88, 14, 103, 167, 58, 122, 146, 250, 216, 102, 207, 153, 185, 74, 182, 103, 89, 162, 151, 100,
			181, 113, 130, 31, 171, 174, 46, 139, 29, 123, 54, 228, 29, 0, 0, 0, 0,
		]);
		const mockProvider = new MockPasskeySigner({
			sk: sk,
			pk: pk,
			authenticatorData: authenticatorData,
		});
		const signer = await PasskeyKeypair.getPasskeyInstance(mockProvider);
		const testMessage = new TextEncoder().encode('Hello world!');
		const { signature } = await signer.signPersonalMessage(testMessage);

		// Verify signature against pubkey.
		const publicKey = signer.getPublicKey();
		const isValid = await publicKey.verifyPersonalMessage(testMessage, signature);
		expect(isValid).toBe(true);

		// Parsed signature as expected.
		const parsed = parseSerializedPasskeySignature(signature);
		expect(parsed.signatureScheme).toEqual('Passkey');
		expect(parsed.publicKey!).toEqual(pk);
		expect(new Uint8Array(parsed.authenticatorData!)).toEqual(authenticatorData);

		const messageBytes = bcs.vector(bcs.u8()).serialize(testMessage).toBytes();
		const intentMessage = messageWithIntent('PersonalMessage', messageBytes);
		const digest = blake2b(intentMessage, { dkLen: 32 });
		const clientDataJSON = {
			type: 'webauthn.get',
			challenge: Buffer.from(digest).toString('base64'),
			origin: 'https://www.sui.io',
			crossOrigin: false,
		};
		expect(parsed.clientDataJson).toEqual(JSON.stringify(clientDataJSON));
	});

	it('should sign a transaction and verify against pubkey', async () => {
		const sk = secp256r1.utils.randomPrivateKey();
		const pk = secp256r1.getPublicKey(sk);
		const authenticatorData = new Uint8Array([
			88, 14, 103, 167, 58, 122, 146, 250, 216, 102, 207, 153, 185, 74, 182, 103, 89, 162, 151, 100,
			181, 113, 130, 31, 171, 174, 46, 139, 29, 123, 54, 228, 29, 0, 0, 0, 0,
		]);
		const mockProvider = new MockPasskeySigner({
			sk: sk,
			pk: pk,
			authenticatorData: authenticatorData,
		});
		const signer = await PasskeyKeypair.getPasskeyInstance(mockProvider);

		const messageBytes = fromBase64(
			'AAABACACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgEBAQABAABnEUWt6SNz7OPa4hXLyCw9tI5Y7rNxhh5DFljH1jLT6QEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAIMqiyOLCIblSqii0TkS8PjMoj3tmA7S24hBMyonz2Op/ZxFFrekjc+zj2uIVy8gsPbSOWO6zcYYeQxZYx9Yy0+noAwAAAAAAAICWmAAAAAAAAA==',
		);
		const intentMessage = messageWithIntent('TransactionData', messageBytes);
		const digest = blake2b(intentMessage, { dkLen: 32 });
		const clientDataJSON = {
			type: 'webauthn.get',
			challenge: Buffer.from(digest).toString('base64'),
			origin: 'https://www.sui.io',
			crossOrigin: false,
		};
		const clientDataJSONString = JSON.stringify(clientDataJSON);

		// Sign the test message.
		const { signature } = await signer.signTransaction(messageBytes);

		// Verify signature against pubkey.
		const publicKey = signer.getPublicKey();
		let isValid = await publicKey.verifyTransaction(messageBytes, signature);
		expect(isValid).toBe(true);

		// Parsed signature as expected.
		const parsed = parseSerializedPasskeySignature(signature);
		expect(parsed.signatureScheme).toEqual('Passkey');
		expect(parsed.publicKey!).toEqual(pk);
		expect(new Uint8Array(parsed.authenticatorData!)).toEqual(authenticatorData);
		expect(parsed.clientDataJson).toEqual(clientDataJSONString);

		// Case 1: passkey returns a signature on wrong digest, fails to verify.
		const mockProviderWrongDigest = new MockPasskeySigner({
			sk: sk,
			pk: pk,
			authenticatorData: authenticatorData,
			changeDigest: true,
		});
		const signerWrongDigest = await PasskeyKeypair.getPasskeyInstance(mockProviderWrongDigest);

		const { signature: wrongSignature } = await signerWrongDigest.signTransaction(messageBytes);
		isValid = await publicKey.verifyTransaction(messageBytes, wrongSignature);
		expect(isValid).toBe(false);

		// Case 2: passkey returns wrong type on client data json, fails to verify.
		const mockProviderWrongClientDataJson = new MockPasskeySigner({
			sk: sk,
			pk: pk,
			authenticatorData: authenticatorData,
			changeClientDataJson: true,
		});
		const signerWrongClientDataJson = await PasskeyKeypair.getPasskeyInstance(
			mockProviderWrongClientDataJson,
		);
		const { signature: wrongSignature2 } =
			await signerWrongClientDataJson.signTransaction(intentMessage);
		isValid = await publicKey.verifyTransaction(messageBytes, wrongSignature2);
		expect(isValid).toBe(false);

		// Case 3: passkey returns mismatched authenticator data, fails to verify.
		const mockProviderWrongAuthenticatorData = new MockPasskeySigner({
			sk: sk,
			pk: pk,
			authenticatorData: authenticatorData,
			changeAuthenticatorData: true,
		});
		const signerWrongAuthenticatorData = await PasskeyKeypair.getPasskeyInstance(
			mockProviderWrongAuthenticatorData,
		);
		const { signature: wrongSignature3 } =
			await signerWrongAuthenticatorData.signTransaction(intentMessage);
		isValid = await publicKey.verifyTransaction(messageBytes, wrongSignature3);
		expect(isValid).toBe(false);

		// Case 4: passkey returns a signature from a mismatch secret key, fails to verify.
		const mockProviderWrongSignature = new MockPasskeySigner({
			sk: sk,
			pk: pk,
			authenticatorData: authenticatorData,
			changeSignature: true,
		});
		const signerWrongSignature = await PasskeyKeypair.getPasskeyInstance(
			mockProviderWrongSignature,
		);
		const { signature: wrongSignature4 } =
			await signerWrongSignature.signTransaction(intentMessage);
		isValid = await publicKey.verifyTransaction(messageBytes, wrongSignature4);
		expect(isValid).toBe(false);
	});

	it('should verify a transaction from rust implementation', async () => {
		// generated test vector from `test_passkey_authenticator` in crates/sui-types/src/unit_tests/passkey_authenticator_test.rs
		let sig = fromBase64(
			'BiVYDmenOnqS+thmz5m5SrZnWaKXZLVxgh+rri6LHXs25B0AAAAAgwF7InR5cGUiOiJ3ZWJhdXRobi5nZXQiLCJjaGFsbGVuZ2UiOiJ4NkszMGNvSGlGMF9iczVVVjNzOEVfcGNPNkhMZ0xBb1A3ZE1uU0U5eERNIiwib3JpZ2luIjoiaHR0cHM6Ly93d3cuc3VpLmlvIiwiY3Jvc3NPcmlnaW4iOmZhbHNlfWICAJqKTgco/tSNg4BuVg/f3x+I8NLYN6QqvxHahKNe0PIhBe3EuhfZf8OL4hReW8acT1TVwmPMcnv4SWiAHaX2dAKBYTKkrLK2zLcfP/hD1aiAn/E0L3XLC4epejnzGRhTuA==',
		);
		let txBytes = fromBase64(
			'AAABACACAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgEBAQABAAAt3HtjT61oHCWWztGfhSC2ianNwi6LL2eOLPvZTdJWMgEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAIMqiyOLCIblSqii0TkS8PjMoj3tmA7S24hBMyonz2Op/Ldx7Y0+taBwlls7Rn4UgtompzcIuiy9njiz72U3SVjLoAwAAAAAAAICWmAAAAAAAAA==',
		);
		const parsed = parseSerializedPasskeySignature(sig);
		expect(parsed.signatureScheme).toEqual('Passkey');
		const pubkey = new PasskeyPublicKey(parsed.publicKey!);
		const isValid = await pubkey.verifyTransaction(txBytes, sig);
		expect(isValid).toBe(true);
	});

	it('should verify a transaction from a real passkey output', async () => {
		// generated test vector from a real iphone passkey output from broswer app: https://github.com/joyqvq/sui-webauthn-poc
		let sig = fromBase64(
			'BiVJlg3liA6MaHQ0Fw9kdmBbj+SuuaKGMseZXPO6gx2XYx0AAAAAhgF7InR5cGUiOiJ3ZWJhdXRobi5nZXQiLCJjaGFsbGVuZ2UiOiJZRG9vQ2RGRnRLLVJBZ3JzaUZqM1hpU1VPQ2pzWXJPWnRGcHVISGhvNDhZIiwib3JpZ2luIjoiaHR0cDovL2xvY2FsaG9zdDo1MTczIiwiY3Jvc3NPcmlnaW4iOmZhbHNlfWIChCx2fLGV+dwNRbTqfCvii70DMj1HiHij5oR9KjZmFMpGQJz3l0ZsNpi0zGQtw81Hj+X+CSshhkcteCzVOJlpKAN2ZM3l9Wxn5TYJFdHc9VphEGzoyTTOfUjpZ7fQV2gt6A==',
		);
		let txBytes = fromBase64(
			'AAAAAFTTJ1JTZKCS6Q6aQS2bkY5gsmP//JTTwIzqsKqnltvLAS6VBPgonu3+e2qJUje77aMw0hTzv7mfKxBglq17ccifBgIAAAAAAAAgb2Je8hW/vUH9otcR+oc1RdjZ2W2oaCNgMu0gTpAVfbNU0ydSU2SgkukOmkEtm5GOYLJj//yU08CM6rCqp5bby+gDAAAAAAAAgIQeAAAAAAAA',
		);
		const parsed = parseSerializedPasskeySignature(sig);
		expect(parsed.signatureScheme).toEqual('Passkey');
		const pubkey = new PasskeyPublicKey(parsed.publicKey!);
		const isValid = await pubkey.verifyTransaction(txBytes, sig);
		expect(isValid).toBe(true);
	});
});
