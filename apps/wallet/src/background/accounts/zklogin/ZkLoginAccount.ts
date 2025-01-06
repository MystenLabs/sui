// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import networkEnv from '_src/background/NetworkEnv';
import { API_ENV, type NetworkEnvType } from '_src/shared/api-env';
import { deobfuscate, obfuscate } from '_src/shared/cryptography/keystore';
import { getSuiClient } from '_src/shared/sui-client';
import { fromExportedKeypair } from '_src/shared/utils/from-exported-keypair';
import { toSerializedSignature, type PublicKey } from '@mysten/sui/cryptography';
import { normalizeSuiAddress } from '@mysten/sui/utils';
import {
	computeZkLoginAddress,
	genAddressSeed,
	getZkLoginSignature,
	jwtToAddress,
	type ComputeZkLoginAddressOptions,
} from '@mysten/sui/zklogin';
import { blake2b } from '@noble/hashes/blake2b';
import { decodeJwt } from 'jose';

import { addNewAccounts, getAccountsByAddress } from '..';
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
	ephemeralKeyPair: string;
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

async function hasTransactionHistory(address: string): Promise<boolean> {
	const rpc = getSuiClient({ env: API_ENV.mainnet, customRpcUrl: null });
	const [txnIds, fromTxnIds] = await Promise.all([
		rpc.queryTransactionBlocks({
			filter: {
				ToAddress: address!,
			},
			limit: 1,
		}),
		rpc.queryTransactionBlocks({
			filter: {
				FromAddress: address!,
			},
			limit: 1,
		}),
	]);

	return !!txnIds.data.length || !!fromTxnIds.data.length;
}

type CreateNewZkLoginAccountResponseItem = Omit<ZkLoginAccountSerialized, 'id'>;

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
	}): Promise<CreateNewZkLoginAccountResponseItem[]> {
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

		const baseAddressComputationParams: ComputeZkLoginAddressOptions = {
			claimName,
			claimValue,
			iss: decodedJWT.iss,
			aud,
			userSalt: BigInt(salt),
		};
		const legacyAddress = computeZkLoginAddress({
			...baseAddressComputationParams,
			legacyAddress: true,
		});
		const nonLegacyAddress = computeZkLoginAddress({
			...baseAddressComputationParams,
			legacyAddress: false,
		});

		const ret: CreateNewZkLoginAccountResponseItem[] = [];

		const accountData: Omit<CreateNewZkLoginAccountResponseItem, 'address'> = {
			type: 'zkLogin',
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

		// By default, always import the legacy address. If the legacy and
		// non-legacy addresses differ, only import the non-legacy address if it
		// has already been used.
		ret.push({
			...accountData,
			address: legacyAddress,
		});
		if (normalizeSuiAddress(legacyAddress) !== normalizeSuiAddress(nonLegacyAddress)) {
			if (await hasTransactionHistory(nonLegacyAddress)) {
				ret.push({
					...accountData,
					address: nonLegacyAddress,
					nickname: accountData.nickname ? `${accountData.nickname} (address 2)` : null,
				});
			}
		}

		return ret;
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

	async signData(data: Uint8Array): Promise<string> {
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
				activeNetwork,
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

		const activeNetwork = await networkEnv.getActiveNetwork();
		const credentialsData: CredentialData = {
			ephemeralKeyPair: ephemeralKeyPair.getSecretKey(),
			minEpoch: Number(epoch),
			maxEpoch,
			network: activeNetwork,
			randomness: randomness.toString(),
			jwt,
		};

		const ephemeralValue = (await this.getEphemeralValue()) || {};
		ephemeralValue[serializeNetwork(credentialsData.network)] = credentialsData;
		await this.setEphemeralValue(ephemeralValue);

		await this.onUnlocked();

		// On re-auth, we check if the account for the complementary
		// legacy/non-legacy address needs to be imported. Additionally, if the
		// complementary account has been imported before and is unlocked, we
		// update its credentials. This sync does not need to block the login
		// process.
		this.#syncAlternateAccount(jwt, credentialsData);

		return credentialsData;
	}

	async #syncAlternateAccount(jwt: string, credentialsData: CredentialData) {
		const salt = await fetchSalt(jwt);
		const legacyAddress = jwtToAddress(jwt, salt, true);
		const nonLegacyAddress = jwtToAddress(jwt, salt, false);
		const decodedJWT = decodeJwt(jwt);

		// if they are the same, do nothing
		if (legacyAddress === nonLegacyAddress) {
			return;
		}

		const { id, ...currentAccount } = await this.getStoredData();

		const alternateAddress =
			currentAccount.address === legacyAddress ? nonLegacyAddress : legacyAddress;
		const alternateAccountIsLegacy = alternateAddress === legacyAddress;
		const [alternateAccount] = await getAccountsByAddress(alternateAddress);

		// if account exists do nothing
		if (alternateAccount) return;

		// If the account is a non-legacy account and has no transaction history, do nothing
		if (!alternateAccountIsLegacy && !hasTransactionHistory(alternateAddress)) {
			return;
		}

		const suffix = alternateAccountIsLegacy ? '' : ' (address 2)';

		await addNewAccounts([
			{
				...currentAccount,
				selected: false,
				createdAt: Date.now(),
				address: alternateAddress,
				nickname: decodedJWT.email ? decodedJWT.email + suffix : currentAccount.nickname + suffix,
				lastUnlockedOn: null,
			},
		]);
	}

	async #generateProofs(
		jwt: string,
		randomness: bigint,
		maxEpoch: number,
		ephemeralPublicKey: PublicKey,
		network: NetworkEnvType,
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
			network,
		});
	}
}
