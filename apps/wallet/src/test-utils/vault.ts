// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromExportedKeypair } from '@mysten/sui.js';
import { mnemonicToSeedHex } from '@mysten/sui.js/cryptography';

import { EPHEMERAL_PASSWORD_KEY, EPHEMERAL_VAULT_KEY } from '_src/background/keyring/VaultStorage';
import { toEntropy } from '_src/shared/utils/bip39';

import type { Keypair } from '@mysten/sui.js';

export const testMnemonic =
	'loud eye weather change muffin brisk episode dance mirror smart image energy';
export const testMnemonicSeedHex = mnemonicToSeedHex(testMnemonic);
export const testEntropySerialized = '842a27e29319123892f9ba8d9991c525';
export const testEntropy = toEntropy(testEntropySerialized);
export const testEd25519SerializedLegacy = Object.freeze({
	schema: 'ED25519',
	privateKey:
		'a3R0jvXpEziZLHsbX1DogdyGm8AK87HScEK+JJHwaV99nEpOfYblbYS3ci9wP2DT5YZtE3e4v/HBsN39kRz60A==',
} as const);

export const testEd25519Legacy = fromExportedKeypair(testEd25519SerializedLegacy);
export const testEd25519AddressLegacy = testEd25519Legacy.getPublicKey().toSuiAddress();

export const testEd25519Serialized = Object.freeze({
	schema: 'ED25519',
	privateKey: 'a3R0jvXpEziZLHsbX1DogdyGm8AK87HScEK+JJHwaV8=',
} as const);
export const testEd25519 = fromExportedKeypair(testEd25519Serialized);
export const testEd25519Address = testEd25519.getPublicKey().toSuiAddress();

export const testSecp256k1Serialized = Object.freeze({
	schema: 'Secp256k1',
	privateKey: '4DD3CUtZvbc9Ur69tTvKaLeIDptxNa9qZcpkyXWjVGY=',
} as const);
export const testSecp256k1 = fromExportedKeypair(testSecp256k1Serialized);
export const testSecp256k1Address = testSecp256k1.getPublicKey().toSuiAddress();
type TestDataVault = typeof testDataVault1;
/**
 * A test vault with 2 keypairs
 */
export const testDataVault1 = Object.freeze({
	mnemonic: testMnemonic as string,
	testMnemonicSeedHex,
	entropy: testEntropy,
	entropySerialized: testEntropySerialized as string,
	keypairs: [testEd25519, testSecp256k1] as Keypair[],
	password: '12345' as string,
	encrypted: {
		v0: '{"data":"58d/PfOjiRO4Hl0o9w5rFvCm8t5NmaDbt/DjgSkYw1gntJSgEzj0bJVbcd4nWTvZuQMG2EnRzsvaOkd3OGuJ7noGedS45xcoW717XWBWEcWFFT77E9nnut/9Q5GrSQ==","iv":"29RBia36PI4eAOs+BLEF3Q==","salt":"cSLTzKEUE5hMRshgl/qD8Tr9wWIcCdCdmejakMIjZNk="}',
		v1: {
			v: 1 as const,
			data: '{"data":"he6cFDl5Q1k71tmE/JRunc6GZElKVa+Ai7c2XAPttF/8Y1wkUN/zGs1X8XPK6x2VPlY=","iv":"mPV3Ov0doeEfEiREHRwj6A==","salt":"oocRfTUxBP/KcNgeeXO3vWcZXdX+uadF76LfU0iyD0o="}',
		},
		v2: {
			v: 2 as const,
			data: '{"data":"duteNkmpItSCH53t2qJB2DS2i0tmavGhVHf5zBZP+2C+2dtsrZ8MWcAh2V7HgKJjPJ5sqiZf/ZULa0qtSdYKDhPTXNQNe14Q0IXza+6McUBZIzscWVzRkiSPoQLz72rOiIswgtBOW8pmn4tFlkApClIksRVeENJzkHPFOz8MqQWzipXVXpcYzv0lpBgQtOm1H+8ArAD+TATM1ggvgv9WXvDvsKqBPO6n1+fLysDqT6OQUoLZoGtvDxrAD/50bnOn5tcASVF5P2IeHVlep/fjHY8dL9f8elbwtA42FDGbXv8vKnSIRIGWNyqjRpkiMxjQwibBEOAyyl15Xjmn5ydyHUmXhu+TXy5SRFANy8Dy/MX0nxRGAoH+RCE8mGnMZJcsn/cdm0ZQ9YMuZB0ng9lCGRpkKONOmNfeeM4nirAsPbQ0f05DwCCzIp1jHQJuPEgy/OwJFYWQIBeAHsiBD9Ivi0+WIQC3z1hBA9yVmL1nThQ700InZOK2qCwuTJOpd/FOkqnO/94AYmq1v/t4HasptkjpvuxJOFxB6X0DwPY=","iv":"pW8cbFYE1mbAC0Nml4d35A==","salt":"s1Qfqw+7pJx04zDfRsFHGIu9Xv9LTizvTDRNOKhFAEs="}',
		},
	},
	sessionStorage: {
		[EPHEMERAL_PASSWORD_KEY]:
			'156790a94d9f9b10ca6f61748ef5f048f66d8bc24f55482333c4d74b97d997b889ac2be54f481eeb6f76a51e176ded15ba884cd9aaf48679fb9caf0516e72e19',
		[EPHEMERAL_VAULT_KEY]: {
			v: 2 as const,
			data: '{"data":"DTFsXKIEPXeIMt2QR5CUw5rHIqY1A2rhSwerk+H7ZssiloSgkzBEFVA2OD8KGn3hMDsJz7QTnzoTeZ7yg/Z2eJwtEqqQO9UaGyTqae9CowhGfYPrlF3HBtLIfPzaSQk/77gnq/059MmBbwrIngc+fZCArmgm3VHx3Emggm/I1aT5vhsPR3RQnW5TTKjelj095VXFSYrWhBgUWFXlt89/5Vq+SxOZsEkD5HHXkdb5Ped8Jwa9Ugsv7qp3gMMtlGOil55X5qfuexXJ7C2MrOcG4ynMo8Zv82duDxnbdGDRkXCcq/0DGgJUdQp4ApTKhtPBdLJ6XMYDJt6edBaBbuNJZOLyY4hNQUkW6kB3z2K9ZarnBm4Er9c4xrQwg6rbzyEHcqER","iv":"+/NhgTYIk2/QxVlf7DOAzg==","salt":"vUIMdz0Jmg6noKTCJqdV7sI5yCQBMge9TKqR2awR2FY="}',
		},
	},
});
/**
 * A test vault with no keypairs
 */
export const testDataVault2: TestDataVault = Object.freeze({
	mnemonic: testMnemonic,
	testMnemonicSeedHex,
	entropy: testEntropy,
	entropySerialized: testEntropySerialized,
	keypairs: [],
	password: '12345',
	encrypted: {
		v0: testDataVault1.encrypted.v0,
		v1: testDataVault1.encrypted.v1,
		v2: {
			v: 2 as const,
			data: '{"data":"KYIbJX9kKqFXoG5cxHluU6dcmaYtuAoJNaJ++RxBBJX3OapJluLj9dJ+xkUy8bm63jxATyfE2RzRKYRnxzn5uLIkpBgXvh46nIumKM9ehat2IZTCgAxA8/RN5QLh59+4TeNpMv8CwnNkNLPTA1Ve7bXI5uhv3Kd2xQ1n1VvqsY2xrt8QROyESQNRpmTec3dOAzA5U+ztoXfvp5itKLDVTcAgeceNwKgR2qdu7QF45yKTDWDCxqrPMHPuYnxTq2iJ9EgYLMpvMIoZ3nXcrP1/4gSI3idwA+rma3j+uVQpneDvVp5x7NUajDfyH44fkrN0cOtwRNoVclqDZg==","iv":"W++5n5xGIJJtbhbGUo3LeA==","salt":"PN3JEwTAO5aGi60zmXU0P6b6sWZDX7IdJVudnbHpv6w="}',
		},
	},
	sessionStorage: {
		[EPHEMERAL_PASSWORD_KEY]:
			'bfd5de9e215ed0c80ab8869c7e53bb29f080d2215c0efe1ebc5754b4012f5ccb5915c25d31308cc1a24a5ee1dda426357638b5d47124e5eb829e5ad14e26124b',
		[EPHEMERAL_VAULT_KEY]: {
			v: 2 as const,
			data: '{"data":"5Cc22P3DMmsJ7q/RFhmuh8+FeK2S5r/K4hzFzNoha+ANFTkaNjbqn+gOjOoG0q67BHItyI4FoCm9isgVfBucvXVJrLcn85Ash1mS6yMDrnt3Zk+i","iv":"Y7X+yW0jEMYDLdP8rBsblQ==","salt":"Af17/8ya+n9+Fk21JKgQAHwckAcg3OiLMkwzQNz27bQ="}',
		},
	},
});
