// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	type SerializedSignature,
	type ExportedKeypair,
	SIGNATURE_SCHEME_TO_FLAG,
	toSerializedSignature,
} from '@mysten/sui.js/cryptography';
import { fromB64, toB64 } from '@mysten/sui.js/utils';
import { computeZkAddress, zkBcs } from '@mysten/zklogin';
import { blake2b } from '@noble/hashes/blake2b';
import { toBigIntBE } from 'bigint-buffer';
import { decodeJwt } from 'jose';
import { getCurrentEpoch } from './current-epoch';
import { type ZkProvider } from './providers';
import {
	type PartialZkSignature,
	createPartialZKSignature,
	fetchPin,
	prepareZKLogin,
	zkLogin,
} from './utils';
import {
	Account,
	type SerializedUIAccount,
	type SigningAccount,
	type SerializedAccount,
} from '../Account';
import networkEnv from '_src/background/NetworkEnv';
import { type NetworkEnvType } from '_src/shared/api-env';
import { deobfuscate, obfuscate } from '_src/shared/cryptography/keystore';
import { fromExportedKeypair } from '_src/shared/utils/from-exported-keypair';

type SessionStorageData = {
	ephemeralKeyPair: ExportedKeypair;
	proofs: PartialZkSignature;
	minEpoch: number;
	maxEpoch: number;
	network: NetworkEnvType;
};

type JwtSerializedClaims = {
	email: string;
	fullName: string | null;
	firstName: string | null;
	lastName: string | null;
	picture: string | null;
	aud: string;
	iss: string;
	sub: string;
};

export interface ZkAccountSerialized extends SerializedAccount {
	type: 'zk';
	provider: ZkProvider;
	/**
	 * the pin used to create the account obfuscated
	 */
	pin: string;
	/**
	 * obfuscated data that contains user info as it was in jwt
	 */
	claims: string;
}

export interface ZkAccountSerializedUI extends SerializedUIAccount {
	type: 'zk';
	email: string;
	picture: string | null;
}

export function isZkAccountSerializedUI(
	account: SerializedUIAccount,
): account is ZkAccountSerializedUI {
	return account.type === 'zk';
}

export class ZkAccount
	extends Account<ZkAccountSerialized, SessionStorageData>
	implements SigningAccount
{
	readonly canSign = true;
	readonly unlockType = 'password' as const;

	static async createNew({
		provider,
	}: {
		provider: ZkProvider;
	}): Promise<Omit<ZkAccountSerialized, 'id'>> {
		const jwt = await zkLogin({ provider, prompt: 'select_account' });
		const { pin } = await fetchPin(jwt);
		const decodedJWT = decodeJwt(jwt);
		if (
			!decodedJWT.sub ||
			!decodedJWT.iss ||
			!decodedJWT.aud ||
			!decodedJWT.email ||
			typeof decodedJWT.email !== 'string'
		) {
			throw new Error('Missing jwt data');
		}
		if (Array.isArray(decodedJWT.aud)) {
			throw new Error('Not supported aud. Aud is an array, string was expected.');
		}
		const aud = decodedJWT.aud;
		const claims: JwtSerializedClaims = {
			email: decodedJWT.email,
			fullName: String(decodedJWT.name || '') || null,
			firstName: String(decodedJWT.given_name || '') || null,
			lastName: String(decodedJWT.family_name || '') || null,
			picture: String(decodedJWT.picture || '') || null,
			aud,
			iss: decodedJWT.iss,
			sub: decodedJWT.sub,
		};
		return {
			type: 'zk',
			address: computeZkAddress({
				claimName: 'sub',
				claimValue: decodedJWT.sub,
				iss: decodedJWT.iss,
				aud,
				userPin: BigInt(pin),
			}),
			claims: await obfuscate(claims),
			pin: await obfuscate(pin),
			provider,
			publicKey: null,
			lastUnlockedOn: null,
		};
	}

	static isOfType(serialized: SerializedAccount): serialized is ZkAccountSerialized {
		return serialized.type === 'zk';
	}

	constructor({ id, cachedData }: { id: string; cachedData?: ZkAccountSerialized }) {
		super({ type: 'zk', id, cachedData });
	}

	async lock(allowRead = false): Promise<void> {
		await this.clearEphemeralValue();
		await this.onLocked(allowRead);
	}

	async isLocked(): Promise<boolean> {
		const credentials = await this.getEphemeralValue();
		if (!credentials) {
			return true;
		}
		const { maxEpoch, network } = credentials;
		const currentNetwork = await networkEnv.getActiveNetwork();
		if (
			currentNetwork.env !== network.env ||
			currentNetwork.customRpcUrl !== network.customRpcUrl
		) {
			await this.lock(true);
			return true;
		}
		return (await getCurrentEpoch()) > maxEpoch;
	}

	async unlock() {
		const { provider, claims, pin: obfuscatedPin } = await this.getStoredData();
		const pin = await deobfuscate<string>(obfuscatedPin);
		const { email, sub, aud, iss } = await deobfuscate<JwtSerializedClaims>(claims);
		const epoch = await getCurrentEpoch();
		const { ephemeralKeyPair, nonce, randomness, maxEpoch } = prepareZKLogin(Number(epoch));
		const jwt = await zkLogin({ provider, nonce, loginHint: sub });
		const decodedJWT = decodeJwt(jwt);
		if (
			decodedJWT.aud !== aud ||
			decodedJWT.email !== email ||
			decodedJWT.sub !== sub ||
			decodedJWT.iss !== iss
		) {
			throw new Error("Logged in account doesn't match with saved account");
		}
		const proofs = await createPartialZKSignature({
			jwt,
			ephemeralPublicKey: toBigIntBE(Buffer.from(ephemeralKeyPair.getPublicKey().toRawBytes())),
			userPin: BigInt(pin),
			jwtRandomness: randomness,
			keyClaimName: 'sub',
			maxEpoch,
		});
		await this.setEphemeralValue({
			ephemeralKeyPair: await ephemeralKeyPair.export(),
			minEpoch: Number(epoch),
			maxEpoch,
			proofs,
			network: await networkEnv.getActiveNetwork(),
		});
		await this.onUnlocked();
	}

	async toUISerialized(): Promise<ZkAccountSerializedUI> {
		const { address, publicKey, type, claims } = await this.getStoredData();
		const { email, picture } = await deobfuscate<JwtSerializedClaims>(claims);
		return {
			id: this.id,
			type,
			address,
			publicKey,
			isLocked: await this.isLocked(),
			lastUnlockedOn: await this.lastUnlockedOn,
			email,
			picture,
		};
	}

	async signData(data: Uint8Array): Promise<SerializedSignature> {
		const digest = blake2b(data, { dkLen: 32 });
		if (await this.isLocked()) {
			// check is locked to handle cases of different network, current epoch higher than max epoch etc.
			throw new Error('Account is locked');
		}
		const credentials = await this.getEphemeralValue();
		if (!credentials) {
			// checking the isLocked above should catch this but keep it just in case
			throw new Error('Account is locked');
		}
		const { ephemeralKeyPair, proofs, maxEpoch } = credentials;
		const keyPair = fromExportedKeypair(ephemeralKeyPair);
		const userSignature = toSerializedSignature({
			signature: await keyPair.sign(digest),
			signatureScheme: keyPair.getKeyScheme(),
			publicKey: keyPair.getPublicKey(),
		});
		const bytes = zkBcs
			.ser(
				'ZkSignature',
				{
					inputs: proofs,
					max_epoch: maxEpoch,
					user_signature: fromB64(userSignature),
				},
				{ maxSize: 2048 },
			)
			.toBytes();
		const signatureBytes = new Uint8Array(bytes.length + 1);
		signatureBytes.set([SIGNATURE_SCHEME_TO_FLAG['Zk']]);
		signatureBytes.set(bytes, 1);
		return toB64(signatureBytes);
	}
}
