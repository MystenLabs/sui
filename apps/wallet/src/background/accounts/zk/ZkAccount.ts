// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SerializedSignature, type ExportedKeypair } from '@mysten/sui.js/cryptography';
import { toBigIntBE } from 'bigint-buffer';
import { decodeJwt } from 'jose';
import { type ZkProvider } from './providers';
import { createZKProofs, fetchPin, getAddress, prepareZKLogin, zkLogin } from './utils';
import {
	Account,
	type SerializedUIAccount,
	type SigningAccount,
	type SerializedAccount,
} from '../Account';
import { deobfuscate, obfuscate } from '_src/shared/cryptography/keystore';
import { getActiveNetworkSuiClient } from '_src/shared/sui-client';

type SessionStorageData = { ephemeralKeyPair: ExportedKeypair };

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
		// TODO: verify this can be an array and if so is it fine like this?
		const aud = Array.isArray(decodedJWT.aud) ? decodedJWT.aud.join(' ') : decodedJWT.aud;
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
			address: await getAddress({
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
		// TODO:
		return true;
	}

	async unlock() {
		const { provider, claims, pin: obfuscatedPin } = await this.getStoredData();
		const pin = await deobfuscate<string>(obfuscatedPin);
		const { email, sub, aud, iss } = await deobfuscate<JwtSerializedClaims>(claims);
		const suiClient = await getActiveNetworkSuiClient();
		const { epoch } = await suiClient.getLatestSuiSystemState();
		const { ephemeralKeyPair, nonce, randomness, maxEpoch } = prepareZKLogin(Number(epoch));
		const jwt = await zkLogin({ provider, nonce, loginHint: sub });
		const decodedJWT = decodeJwt(jwt);
		if (
			decodedJWT.aud !== aud ||
			decodedJWT.email !== email ||
			decodedJWT.sub !== sub ||
			decodedJWT.iss !== iss
		) {
			throw new Error("Logged in account doesn't much with saved account");
		}
		const proofs = await createZKProofs({
			jwt,
			ephemeralPublicKey: toBigIntBE(Buffer.from(ephemeralKeyPair.getPublicKey().toRawBytes())),
			userPin: BigInt(pin),
			jwtRandomness: randomness,
			keyClaimName: 'sub',
			maxEpoch,
		});
		console.log({ jwt, proofs });
		// TODO:
	}

	async toUISerialized(): Promise<ZkAccountSerializedUI> {
		const { address, publicKey, type } = await this.getStoredData();
		return {
			id: this.id,
			type,
			address,
			publicKey,
			isLocked: await this.isLocked(),
			lastUnlockedOn: await this.lastUnlockedOn,
		};
	}

	async signData(data: Uint8Array): Promise<SerializedSignature> {
		// TODO:
		return '';
	}
}
