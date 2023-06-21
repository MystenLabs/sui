// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromExportedKeypair } from '@mysten/sui.js';

import { EPHEMERAL_PASSWORD_KEY, EPHEMERAL_VAULT_KEY } from '_src/background/keyring/VaultStorage';
import { toEntropy } from '_src/shared/utils/bip39';

import type { Keypair } from '@mysten/sui.js';

export const testMnemonic =
	'loud eye weather change muffin brisk episode dance mirror smart image energy';
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
			data: '{"data":"2A1AuR8RUzdfcrKuAm+AgOsCHkA+6XHpxHI8SrWKSmzzCyHbUdxPXI65lR55+uHPVKi9Sk9q+wTaM3Dgr9hzUFJ2wX43bcjZxhBJ2Xo/RqNI5tLQWyx4Y6xKSrB8MjbDf/Zq29AEArIPOoTz36Tsr8GR0m92y/9xAtskctOIlQKKiNgvZ8z3eN7AfeO0PgTJVTiEkBxruqzL0A9XDNW4xNySKkig5UbzfhNXa1pBieDyXWcmpNnOe+7RVxXMZX9FAro31+KI5SexoAJ6TE3L/hv9b0zQgND1otAjPu6AB5d3VG6BaOKlEHxqBeoGNya4iCoYSg0CB6kViGCwhWyjiylkABJ3Q++dDxxXyCP2nuw0rxbiB6VoElEEIwaraVS/c8Q0","iv":"bBKS/FT16UqyHPNnjnBS5Q==","salt":"1ViXZMKEQ2kxwq761j2SY8SHgHxQ8kiWil8hd3Ni5CI="}',
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
	entropy: testEntropy,
	entropySerialized: testEntropySerialized,
	keypairs: [],
	password: '12345',
	encrypted: {
		v0: testDataVault1.encrypted.v0,
		v1: testDataVault1.encrypted.v1,
		v2: {
			v: 2 as const,
			data: '{"data":"5Oua+5DkH7OWWkvNseCqAECC9PF6Csxl5E4zDEdV5/uthNgI3/c1WjZCswsYEXMxeBoxnfUIzjKLthAHKvZOdYbvISzlCjtXmDoWeRnFvrWiXKWV","iv":"K5uVmJUkArzVzU58VGzIuw==","salt":"IM7Y1aRpJ5WQajQbmdGHI8+D2cZXHblEc58aoHfISfg="}',
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
