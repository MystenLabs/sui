// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { fromBase64 } from '@mysten/bcs';
import { describe, expect, it } from 'vitest';

import { SuiGraphQLClient } from '../../src/graphql';
import { Ed25519Keypair } from '../../src/keypairs/ed25519';
import { Secp256k1Keypair } from '../../src/keypairs/secp256k1';
import { Secp256r1Keypair } from '../../src/keypairs/secp256r1';
import { MultiSigPublicKey } from '../../src/multisig/publickey';
import { verifyPersonalMessageSignature } from '../../src/verify';

describe('Verify Signatures', () => {
	// describe('transaction signatures', () => {});
	describe('personal message signatures', () => {
		describe('single signatures', () => {
			describe('Ed25519', () => {
				const keypair = new Ed25519Keypair();
				const address = keypair.getPublicKey().toSuiAddress();
				const message = new TextEncoder().encode('hello world');

				it('verifies valid signatures', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					const publicKey = await verifyPersonalMessageSignature(message, signature);
					expect(publicKey.toSuiAddress()).toBe(address);
				});

				it('verifies signatures against provided address', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					await expect(
						verifyPersonalMessageSignature(message, signature, { address }),
					).resolves.toBeDefined();
				});

				it('fails for invalid signatures', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					const invalidMessage = new TextEncoder().encode('wrong message');
					await expect(verifyPersonalMessageSignature(invalidMessage, signature)).rejects.toThrow();
				});

				it('fails for wrong address', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					const wrongAddress = new Ed25519Keypair().getPublicKey().toSuiAddress();
					await expect(
						verifyPersonalMessageSignature(message, signature, { address: wrongAddress }),
					).rejects.toThrow();
				});
			});

			describe('Secp256k1', () => {
				const keypair = new Secp256k1Keypair();
				const address = keypair.getPublicKey().toSuiAddress();
				const message = new TextEncoder().encode('hello world');

				it('verifies valid signatures', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					const publicKey = await verifyPersonalMessageSignature(message, signature);
					expect(publicKey.toSuiAddress()).toBe(address);
				});

				it('verifies signatures against provided address', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					await expect(
						verifyPersonalMessageSignature(message, signature, { address }),
					).resolves.toBeDefined();
				});

				it('fails for invalid signatures', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					const invalidMessage = new TextEncoder().encode('wrong message');
					await expect(verifyPersonalMessageSignature(invalidMessage, signature)).rejects.toThrow();
				});

				it('fails for wrong address', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					const wrongAddress = new Secp256k1Keypair().getPublicKey().toSuiAddress();
					await expect(
						verifyPersonalMessageSignature(message, signature, { address: wrongAddress }),
					).rejects.toThrow();
				});
			});

			describe('Secp256r1', () => {
				const keypair = new Secp256r1Keypair();
				const address = keypair.getPublicKey().toSuiAddress();
				const message = new TextEncoder().encode('hello world');

				it('verifies valid signatures', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					const publicKey = await verifyPersonalMessageSignature(message, signature);
					expect(publicKey.toSuiAddress()).toBe(address);
				});

				it('verifies signatures against provided address', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					await expect(
						verifyPersonalMessageSignature(message, signature, { address }),
					).resolves.toBeDefined();
				});

				it('fails for invalid signatures', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					const invalidMessage = new TextEncoder().encode('wrong message');
					await expect(verifyPersonalMessageSignature(invalidMessage, signature)).rejects.toThrow();
				});

				it('fails for wrong address', async () => {
					const { signature } = await keypair.signPersonalMessage(message);
					const wrongAddress = new Secp256r1Keypair().getPublicKey().toSuiAddress();
					await expect(
						verifyPersonalMessageSignature(message, signature, { address: wrongAddress }),
					).rejects.toThrow();
				});
			});
		});

		describe('multisig signatures', () => {
			const k1 = new Ed25519Keypair();
			const k2 = new Secp256k1Keypair();
			const k3 = new Secp256r1Keypair();
			const pk1 = k1.getPublicKey();
			const pk2 = k2.getPublicKey();
			const pk3 = k3.getPublicKey();

			it('verifies valid multisig signatures', async () => {
				const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
					threshold: 3,
					publicKeys: [
						{ publicKey: pk1, weight: 1 },
						{ publicKey: pk2, weight: 2 },
						{ publicKey: pk3, weight: 3 },
					],
				});

				const message = new TextEncoder().encode('hello world');
				const sig1 = await k1.signPersonalMessage(message);
				const sig2 = await k2.signPersonalMessage(message);

				const multisig = multiSigPublicKey.combinePartialSignatures([
					sig1.signature,
					sig2.signature,
				]);

				const publicKey = await verifyPersonalMessageSignature(message, multisig);
				expect(publicKey.toSuiAddress()).toBe(multiSigPublicKey.toSuiAddress());
			});

			it('fails for invalid multisig signatures', async () => {
				const multiSigPublicKey = MultiSigPublicKey.fromPublicKeys({
					threshold: 3,
					publicKeys: [
						{ publicKey: pk1, weight: 1 },
						{ publicKey: pk2, weight: 2 },
						{ publicKey: pk3, weight: 3 },
					],
				});

				const message = new TextEncoder().encode('hello world');
				const wrongMessage = new TextEncoder().encode('wrong message');
				const sig1 = await k1.signPersonalMessage(message);
				const sig2 = await k2.signPersonalMessage(message);

				const multisig = multiSigPublicKey.combinePartialSignatures([
					sig1.signature,
					sig2.signature,
				]);

				await expect(verifyPersonalMessageSignature(wrongMessage, multisig)).rejects.toThrow();
			});
		});

		describe('zkLogin signatures', () => {
			const client = new SuiGraphQLClient({ url: 'http://127.0.0.1:9125' });
			// this test assumes the localnet epoch is smaller than 3. it will fail if localnet has ran for too long and passed epoch 3.
			// test case generated from `sui keytool zk-login-insecure-sign-personal-message --data "hello" --max-epoch 3`
			const bytes = fromBase64('aGVsbG8='); // the base64 encoding of "hello"
			const testSignature =
				'BQNMNTk1OTM0OTYxMDU0NTQyODY4Mjc1NDY0OTc0NTI4NzkyMTIyMjQ2NjIzMTU1ODY4ODUxNDk4Mzc0Mzc5OTkyNjc4NTMzODAzOTM0OE0xMzg3MDY1NTY1MjI1NjI2MjYzNzgxMjUyNzc0ODg3ODQ2MTg0Njc4MzgwMjY1Njg5NTE3MjAyNjgwNDE0NzQzOTcwNTM1NDgzMDIxNwExAwJNMTEyNzgwMzY0NTU1NjAzNTQwMTY1OTI5NDIxMTg3Mjk2OTQwNzQyMTI4NTUzNjcwODUxNTA2MDY2NTU1OTM1OTYwMzYzMzc1NjIxNjVNMTIxMDg3MjQ3MjQyNjQ3MjUzMTMzMTUwNTY3NjIxNjkxNTgyMzQ2NDIyODQwOTkwNzgwNzM3OTUwMTk4NTE3OTE3MjI1MTkyNTk3NTYCTTE4MDM5NjgwODY4NTY4NzQwNDY4NzU1NTg2NDE4NjI1MTQzMDMyMzM4MzQxODk3MjM2NDE5NzEwOTYxNTcyMTA4MDU4MTc4NjY4MjgxTTExMjE2MjQxMTEzNjYzMjg2OTcxNTQ1MjAwMjI1OTIxMzYzNDkyMjMxMTYwMzgwMDQ3MjQ4NjczNTQyODg0MjQ4Nzc4NzMwNjEwNTM5AgExATADTTIxNTUyNjMyMDI1OTM2NDUyNDQzMTY5MDAzMDUwNDQyMTE1MDE0NzIzMDg5NDkyOTU5MTU5MjM2NTI1OTc1Nzk3OTQ5NzM4NjU0NjI2TTE1MTUxNjExMzkzMTY2NzczMzU1MDQ1ODExODA0Nzg2NTczNDQ4MTc1NzMzODQ2MTQwMjUxNTY2NDg0NjkyMjYxMTUwNDExOTQxODU2ATEod2lhWE56SWpvaWFIUjBjSE02THk5dllYVjBhQzV6ZFdrdWFXOGlMQwI+ZXlKcmFXUWlPaUp6ZFdrdGEyVjVMV2xrSWl3aWRIbHdJam9pU2xkVUlpd2lZV3huSWpvaVVsTXlOVFlpZlFNMjA0MzUzNjY2MDAwMzYzNzU3NDU5MjU5NjM0NTY4NjEzMDc5MjUyMDk0NzAyMTkzMzQwMTg1NjQxNTgxNDg1NDQwMzYxOTYyODQ2NDIeAAAAAAAAAGEA+XrHUDMkMaPswTIFsqIgx3yX6j7IvU1T/1yzw4kjKwjgLL0ZtPQZjc2djX7Q9pFoBdObkSZ6JZ4epiOf05q4BrnG7hYw7z5xEUSmSNsGu7IoT3J0z77lP/zuUDzBpJIA';

			it('verifies valid signatures', async () => {
				const publicKey = await verifyPersonalMessageSignature(bytes, testSignature, {
					client,
					address: '0xc0f0d0e2a2ca8b8d0e4055ec48210ec77d055db353402cda01d7085ba61d3d5c',
				});
				expect(publicKey).toBeDefined();

				expect(publicKey.toSuiAddress()).toBe(
					'0xc0f0d0e2a2ca8b8d0e4055ec48210ec77d055db353402cda01d7085ba61d3d5c',
				);
			});

			it('fails for invalid signatures', async () => {
				const bytes = fromBase64('aGVsbG8=');
				const invalidSignature =
					'BQNMNTk1OTM0OTYxMDU0NTQyODY4Mjc1NDY0OTc0NTI4NzkyMTIyMjQ2NjIzMTU1ODY4ODUxNDk4Mzc0Mzc5OTkyNjc4NTMzODAzOTM0OE0xMzg3MDY1NTY1MjI1NjI2MjYzNzgxMjUyNzc0ODg3ODQ2MTg0Njc4MzgwMjY1Njg5NTE3MjAyNjgwNDE0NzQzOTcwNTM1NDgzMDIxNwExAwJNMTEyNzgwMzY0NTU1NjAzNTQwMTY1OTI5NDIxMTg3Mjk2OTQwNzQyMTI4NTUzNjcwODUxNTA2MDY2NTU1OTM1OTYwMzYzMzc1NjIxNjVNMTIxMDg3MjQ3MjQyNjQ3MjUzMTMzMTUwNTY3NjIxNjkxNTgyMzQ2NDIyODQwOTkwNzgwNzM3OTUwMTk4NTE3OTE3MjI1MTkyNTk3NTYCTTE4MDM5NjgwODY4NTY4NzQwNDY4NzU1NTg2NDE4NjI1MTQzMDMyMzM4MzQxODk3MjM2NDE5NzEwOTYxNTcyMTA4MDU4MTc4NjY4MjgxTTExMjE2MjQxMTEzNjYzMjg2OTcxNTQ1MjAwMjI1OTIxMzYzNDkyMjMxMTYwMzgwMDQ3MjQ4NjczNTQyODg0MjQ4Nzc4NzMwNjEwNTM5AgExATADTTIxNTUyNjMyMDI1OTM2NDUyNDQzMTY5MDAzMDUwNDQyMTE1MDE0NzIzMDg5NDkyOTU5MTU5MjM2NTI1OTc1Nzk3OTQ5NzM4NjU0NjI2TTE1MTUxNjExMzkzMTY2NzczMzU1MDQ1ODExODA0Nzg2NTczNDQ4MTc1NzMzODQ2MTQwMjUxNTY2NDg0NjkyMjYxMTUwNDExOTQxODU2ATEod2lhWE56SWpvaWFIUjBjSE02THk5dllYVjBhQzV6ZFdrdWFXOGlMQwI+ZXlKcmFXUWlPaUp6ZFdrdGEyVjVMV2xrSWl3aWRIbHdJam9pU2xkVUlpd2lZV3huSWpvaVVsTXlOVFlpZlFNMjA0MzUzNjY2MDAwMzYzNzU3NDU5MjU5NjM0NTY4NjEzMDc5MjUyMDk0NzAyMTkzMzQwMTg1NjQxNTgxNDg1NDQwMzYxOTYyODQ2NDIeAAAAAAAAAGEA+XrHUDMkMaPswTIFsqIgx3yX6j7IvU1T/1yzw4kjKwjgLL0ZtPQZjc2djX7Q9pFoBdObkSZ6JZ4epiOf05q4BrnG7hYw7z5xEUSmSNsGu7IoT3J0z77lP/zuUDzBpJIa';

				await expect(
					verifyPersonalMessageSignature(bytes, invalidSignature, { client }),
				).rejects.toThrow();
			});
		});
	});
});
