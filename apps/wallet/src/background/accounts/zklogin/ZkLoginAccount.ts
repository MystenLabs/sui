// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import networkEnv from '_src/background/NetworkEnv';
import { type NetworkEnvType } from '_src/shared/api-env';
import { deobfuscate, obfuscate } from '_src/shared/cryptography/keystore';
import { fromExportedKeypair } from '_src/shared/utils/from-exported-keypair';
import {
	toSerializedSignature,
	type ExportedKeypair,
	type PublicKey,
	type SerializedSignature,
} from '@mysten/sui.js/cryptography';
import { computeZkLoginAddress, genAddressSeed, getZkLoginSignature } from '@mysten/zklogin';
import { blake2b } from '@noble/hashes/blake2b';
import { decodeJwt } from 'jose';

import {
	Account,
	type SerializedAccount,
	type SerializedUIAccount,
	type SigningAccount,
} from '../Account';
import { getCurrentEpoch } from './current-epoch';
import { type ZkLoginProvider } from './providers';
import {
	createPartialZkLoginSignature,
	fetchSalt,
	prepareZkLogin,
	zkLoginAuthenticate,
	type PartialZkLoginSignature,
} from './utils';

type SerializedNetwork = `${NetworkEnvType['env']}_${NetworkEnvType['customRpcUrl']}`;

function serializeNetwork(network: NetworkEnvType): SerializedNetwork {
	return `${network.env}_${network.customRpcUrl}`;
}

type CredentialData = {
	ephemeralKeyPair: ExportedKeypair;
	proofs?: PartialZkLoginSignature;
	minEpoch: number;
	maxEpoch: number;
	network: NetworkEnvType;
	randomness: string;
	jwt: string;
};

type SessionStorageData = Partial<Record<SerializedNetwork, CredentialData>>;

type JwtSerializedClaims = {
	email: string | null;
	fullName: string | null;
	firstName: string | null;
	lastName: string | null;
	picture: string | null;
	aud: string;
	iss: string;
	sub: string;
};

export interface ZkLoginAccountSerialized extends SerializedAccount {
	type: 'zkLogin';
	provider: ZkLoginProvider;
	/**
	 * the salt used to create the account obfuscated
	 */
	salt: string;
	/**
	 * obfuscated data that contains user info as it was in jwt
	 */
	claims: string;
	/**
	 * the addressSeed obfuscated
	 */
	addressSeed: string;
	/**
	 * the name/key of the claim in claims used for the address sub or email
	 */
	claimName: 'sub' | 'email';
	warningAcknowledged?: boolean;
}

export interface ZkLoginAccountSerializedUI extends SerializedUIAccount {
	type: 'zkLogin';
	email: string | null;
	picture: string | null;
	provider: ZkLoginProvider;
	warningAcknowledged: boolean;
}

export function isZkLoginAccountSerializedUI(
	account: SerializedUIAccount,
): account is ZkLoginAccountSerializedUI {
	return account.type === 'zkLogin';
}

export class ZkLoginAccount
	extends Account<ZkLoginAccountSerialized, SessionStorageData>
	implements SigningAccount
{
	readonly canSign = true;
	readonly unlockType = 'password' as const;

	static async createNew({
		provider,
	}: {
		provider: ZkLoginProvider;
	}): Promise<Omit<ZkLoginAccountSerialized, 'id'>> {
		const jwt = await zkLoginAuthenticate({ provider, prompt: true });
		const salt = await fetchSalt(jwt);
		const decodedJWT = decodeJwt(jwt);
		if (!decodedJWT.sub || !decodedJWT.iss || !decodedJWT.aud) {
			throw new Error('Missing jwt data');
		}
		if (Array.isArray(decodedJWT.aud)) {
			throw new Error('Not supported aud. Aud is an array, string was expected.');
		}
		const aud = decodedJWT.aud;
		const claims: JwtSerializedClaims = {
			email: String(decodedJWT.email || '') || null,
			fullName: String(decodedJWT.name || '') || null,
			firstName: String(decodedJWT.given_name || '') || null,
			lastName: String(decodedJWT.family_name || '') || null,
			picture: String(decodedJWT.picture || '') || null,
			aud,
			iss: decodedJWT.iss,
			sub: decodedJWT.sub,
		};
		const claimName = 'sub';
		const claimValue = decodedJWT.sub;
		return {
			type: 'zkLogin',
			address: computeZkLoginAddress({
				claimName,
				claimValue,
				iss: decodedJWT.iss,
				aud,
				userSalt: BigInt(salt),
			}),
			claims: await obfuscate(claims),
			salt: await obfuscate(salt),
			addressSeed: await obfuscate(
				genAddressSeed(BigInt(salt), claimName, claimValue, aud).toString(),
			),
			provider,
			publicKey: null,
			lastUnlockedOn: null,
			selected: false,
			nickname: claims.email || null,
			createdAt: Date.now(),
			claimName,
		};
	}

	static isOfType(serialized: SerializedAccount): serialized is ZkLoginAccountSerialized {
		return serialized.type === 'zkLogin';
	}

	constructor({ id, cachedData }: { id: string; cachedData?: ZkLoginAccountSerialized }) {
		super({ type: 'zkLogin', id, cachedData });
	}

	async lock(allowRead = false): Promise<void> {
		await this.clearEphemeralValue();
		await this.onLocked(allowRead);
	}

	async isLocked(): Promise<boolean> {
		return !(await this.getEphemeralValue());
	}

	async unlock() {
		await this.#doLogin();
	}

	async toUISerialized(): Promise<ZkLoginAccountSerializedUI> {
		const { address, publicKey, type, claims, selected, provider, nickname, warningAcknowledged } =
			await this.getStoredData();
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
			selected,
			nickname,
			isPasswordUnlockable: false,
			provider,
			isKeyPairExportable: false,
			warningAcknowledged: !!warningAcknowledged,
		};
	}

	async signData(data: Uint8Array): Promise<SerializedSignature> {
		const digest = blake2b(data, { dkLen: 32 });
		if (await this.isLocked()) {
			throw new Error('Account is locked');
		}
		const credentials = await this.getEphemeralValue();
		if (!credentials) {
			// checking the isLocked above should catch this but keep it just in case
			throw new Error('Account is locked');
		}
		const activeNetwork = await networkEnv.getActiveNetwork();
		let credentialsData = credentials[serializeNetwork(activeNetwork)];
		const currentEpoch = await getCurrentEpoch();
		// handle cases of different network, current epoch higher than max epoch etc.
		if (!this.#areCredentialsValid(currentEpoch, activeNetwork, credentialsData)) {
			credentialsData = await this.#doLogin();
		}
		const { ephemeralKeyPair, proofs: storedProofs, maxEpoch, jwt, randomness } = credentialsData;
		const keyPair = fromExportedKeypair(ephemeralKeyPair);
		let proofs = storedProofs;
		if (!proofs) {
			proofs = await this.#generateProofs(
				jwt,
				BigInt(randomness),
				maxEpoch,
				keyPair.getPublicKey(),
			);
			credentialsData.proofs = proofs;
			// store the proofs to avoid creating them again
			const newEphemeralValue = await this.getEphemeralValue();
			if (!newEphemeralValue) {
				// this should never happen
				throw new Error('Missing data, account is locked');
			}
			newEphemeralValue[serializeNetwork(activeNetwork)] = credentialsData;
			await this.setEphemeralValue(newEphemeralValue);
		}
		const userSignature = toSerializedSignature({
			signature: await keyPair.sign(digest),
			signatureScheme: keyPair.getKeyScheme(),
			publicKey: keyPair.getPublicKey(),
		});
		const { addressSeed: addressSeedObfuscated } = await this.getStoredData();
		const addressSeed = await deobfuscate<string>(addressSeedObfuscated);

		return getZkLoginSignature({
			inputs: { ...proofs, addressSeed },
			maxEpoch,
			userSignature,
		});
	}

	#areCredentialsValid(
		currentEpoch: number,
		activeNetwork: NetworkEnvType,
		credentials?: CredentialData,
	): credentials is CredentialData {
		if (!credentials) {
			return false;
		}
		const { maxEpoch, network } = credentials;
		return (
			activeNetwork.env === network.env &&
			activeNetwork.customRpcUrl === network.customRpcUrl &&
			currentEpoch <= maxEpoch
		);
	}

	async #doLogin() {
		const { provider, claims } = await this.getStoredData();
		const { sub, aud, iss } = await deobfuscate<JwtSerializedClaims>(claims);
		const epoch = await getCurrentEpoch();
		const { ephemeralKeyPair, nonce, randomness, maxEpoch } = prepareZkLogin(Number(epoch));
		const jwt = await zkLoginAuthenticate({ provider, nonce, loginHint: sub });
		const decodedJWT = decodeJwt(jwt);
		if (decodedJWT.aud !== aud || decodedJWT.sub !== sub || decodedJWT.iss !== iss) {
			throw new Error("Logged in account doesn't match with saved account");
		}
		const ephemeralValue = (await this.getEphemeralValue()) || {};
		const activeNetwork = await networkEnv.getActiveNetwork();
		const credentialsData: CredentialData = {
			ephemeralKeyPair: ephemeralKeyPair.export(),
			minEpoch: Number(epoch),
			maxEpoch,
			network: activeNetwork,
			randomness: randomness.toString(),
			jwt,
		};
		ephemeralValue[serializeNetwork(activeNetwork)] = credentialsData;
		await this.setEphemeralValue(ephemeralValue);
		await this.onUnlocked();
		return credentialsData;
	}

	async #generateProofs(
		jwt: string,
		randomness: bigint,
		maxEpoch: number,
		ephemeralPublicKey: PublicKey,
	) {
		const { salt: obfuscatedSalt, claimName } = await this.getStoredData();
		const salt = await deobfuscate<string>(obfuscatedSalt);
		return await createPartialZkLoginSignature({
			jwt,
			ephemeralPublicKey,
			userSalt: BigInt(salt),
			jwtRandomness: randomness,
			keyClaimName: claimName,
			maxEpoch,
		});
	}
}
