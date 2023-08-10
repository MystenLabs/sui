// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SIGNATURE_SCHEME_TO_FLAG } from '@mysten/sui.js/cryptography';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { SUI_ADDRESS_LENGTH, normalizeSuiAddress } from '@mysten/sui.js/utils';
import { blake2b } from '@noble/hashes/blake2b';
import { bytesToHex, randomBytes } from '@noble/hashes/utils';
import { toBigIntBE } from 'bigint-buffer';
import { base64url } from 'jose';
import {
	poseidon1,
	poseidon2,
	poseidon3,
	poseidon4,
	poseidon5,
	poseidon6,
	poseidon7,
	poseidon8,
	poseidon9,
	poseidon10,
	poseidon11,
	poseidon12,
	poseidon13,
	poseidon14,
	poseidon15,
	poseidon16,
} from 'poseidon-lite';
import Browser from 'webextension-polyfill';
import { bcs } from './bcs';
import { type ZkProvider, zkProviderDataMap } from './providers';
import { fetchWithSentry } from '_src/shared/utils';

const maxKeyClaimNameLength = 40;
const maxKeyClaimValueLength = 256;
const packWidth = 248;
const nonceLength = 27;
const poseidonNumToHashFN = [
	undefined,
	poseidon1,
	poseidon2,
	poseidon3,
	poseidon4,
	poseidon5,
	poseidon6,
	poseidon7,
	poseidon8,
	poseidon9,
	poseidon10,
	poseidon11,
	poseidon12,
	poseidon13,
	poseidon14,
	poseidon15,
	poseidon16,
];
type bit = 0 | 1;

interface UserInfo {
	name: string;
	value: string;
}

function poseidonHash(inputs: (string | number | bigint)[]): bigint {
	const hashFN = poseidonNumToHashFN[inputs.length];
	if (hashFN) {
		return hashFN(inputs);
	} else if (inputs.length <= 32) {
		const hash1 = poseidonHash(inputs.slice(0, 16));
		const hash2 = poseidonHash(inputs.slice(16));
		return poseidonHash([hash1, hash2]);
	} else {
		throw new Error(`Yet to implement: Unable to hash a vector of length ${inputs.length}`);
	}
}

function padWithZeroes<T>(inArr: T[], outCount: number) {
	if (inArr.length > outCount) {
		throw new Error('inArr is big enough');
	}
	const extraZeroes = outCount - inArr.length;
	const arrPadded = inArr.concat(Array(extraZeroes).fill(0));
	return arrPadded;
}

function bigIntArray2Bits(arr: bigint[], intSize: number): bit[] {
	return arr.reduce((bitArray: bit[], n) => {
		const binaryString = n.toString(2).padStart(intSize, '0');
		const bitValues = binaryString.split('').map((bit) => (bit === '1' ? 1 : 0));
		return bitArray.concat(bitValues);
	}, []);
}

function arrayChunk<T>(array: T[], chunkSize: number): T[][] {
	return Array(Math.ceil(array.length / chunkSize))
		.fill(undefined)
		.map((_, index) => index * chunkSize)
		.map((begin) => array.slice(begin, begin + chunkSize));
}

// Pack into an array of chunks each outWidth bits
function pack(inArr: bigint[], inWidth: number, outWidth: number): bigint[] {
	const bits = bigIntArray2Bits(inArr, inWidth);
	const extraBits = bits.length % outWidth === 0 ? 0 : outWidth - (bits.length % outWidth);
	const bitsPadded = bits.concat(Array(extraBits).fill(0));
	if (bitsPadded.length % outWidth !== 0) {
		throw new Error('Invalid logic');
	}
	const packed = arrayChunk(bitsPadded, outWidth).map((chunk) => BigInt('0b' + chunk.join('')));
	return packed;
}

// Pack into exactly outCount chunks of outWidth bits each
function pack2(inArr: bigint[], inWidth: number, outWidth: number, outCount: number): bigint[] {
	const packed = pack(inArr, inWidth, outWidth);
	if (packed.length > outCount) {
		throw new Error('packed is big enough');
	}
	return packed.concat(Array(outCount - packed.length).fill(0));
}

async function mapToField(input: bigint[], inWidth: number) {
	if (packWidth % 8 !== 0) {
		throw new Error('packWidth must be a multiple of 8');
	}
	const numElements = Math.ceil((input.length * inWidth) / packWidth);
	const packed = pack2(input, inWidth, packWidth, numElements);
	return poseidonHash(packed);
}

// Pads a stream of bytes and maps it to a field element
async function mapBytesToField(str: string, maxSize: number) {
	if (str.length > maxSize) {
		throw new Error(`String ${str} is longer than ${maxSize} chars`);
	}
	// Note: Padding with zeroes is safe because we are only using this function to map human-readable sequence of bytes.
	// So the ASCII values of those characters will never be zero (null character).
	const strPadded = padWithZeroes(
		str.split('').map((c) => BigInt(c.charCodeAt(0))),
		maxSize,
	);
	return mapToField(strPadded, 8);
}

async function genAddressSeed(pin: bigint, { name, value }: UserInfo) {
	if (name.length > maxKeyClaimNameLength) {
		throw new Error('Name is too long');
	}
	if (value.length > maxKeyClaimValueLength) {
		throw new Error('Value is too long');
	}
	return poseidonHash([
		await mapBytesToField(name, maxKeyClaimNameLength),
		await mapBytesToField(value, maxKeyClaimValueLength),
		poseidonHash([pin]),
	]);
}

// use custom function because when having a width 20 the library
// returns different results between native and browser computations
function toBufferBE(num: bigint, width: number) {
	const hex = num.toString(16);
	return Buffer.from(hex.padStart(width * 2, '0').slice(-width * 2), 'hex');
}

function generateNonce(ephemeralPublicKey: bigint, maxEpoch: number, randomness: bigint) {
	const eph_public_key_0 = ephemeralPublicKey / 2n ** 128n;
	const eph_public_key_1 = ephemeralPublicKey % 2n ** 128n;
	const bigNum = poseidonHash([eph_public_key_0, eph_public_key_1, maxEpoch, randomness]);
	const Z = toBufferBE(bigNum, 20);
	const nonce = base64url.encode(Z);
	if (nonce.length !== nonceLength) {
		throw new Error(`Length of nonce ${nonce} (${nonce.length}) is not equal to ${nonceLength}`);
	}
	return nonce;
}

export function prepareZKLogin(currentEpoch: number) {
	const maxEpoch = currentEpoch + 2;
	const ephemeralKeyPair = new Ed25519Keypair();
	const randomness = toBigIntBE(Buffer.from(randomBytes(16)));
	const nonce = generateNonce(
		toBigIntBE(Buffer.from(ephemeralKeyPair.getPublicKey().toRawBytes())),
		maxEpoch,
		randomness,
	);
	return {
		ephemeralKeyPair,
		randomness,
		nonce,
		maxEpoch,
	};
}

export async function getAddress({
	claimName,
	claimValue,
	userPin,
	iss,
	aud,
}: {
	claimName: string;
	claimValue: string;
	userPin: bigint;
	iss: string;
	aud: string;
}) {
	const addressSeedBytes = toBufferBE(
		await genAddressSeed(userPin, { name: claimName, value: claimValue }),
		32,
	);
	const addressParamBytes = bcs.ser('AddressParams', { iss, aud }).toBytes();
	const tmp = new Uint8Array(1 + addressSeedBytes.length + addressParamBytes.length);
	tmp.set([SIGNATURE_SCHEME_TO_FLAG['Zk']]);
	tmp.set(addressParamBytes, 1);
	tmp.set(addressSeedBytes, 1 + addressParamBytes.length);
	return normalizeSuiAddress(
		bytesToHex(blake2b(tmp, { dkLen: 32 })).slice(0, SUI_ADDRESS_LENGTH * 2),
	);
}

export async function zkLogin({
	provider,
	nonce,
	loginHint,
	prompt,
}: {
	provider: ZkProvider;
	nonce?: string;
	loginHint?: string;
	prompt?: 'select_account' | 'consent';
}) {
	if (!nonce) {
		nonce = base64url.encode(randomBytes(20));
	}
	const { clientID, url } = zkProviderDataMap[provider];
	const params = new URLSearchParams();
	params.append('client_id', clientID);
	params.append('response_type', 'id_token');
	params.append('redirect_uri', Browser.identity.getRedirectURL());
	params.append('scope', 'openid email profile');
	params.append('nonce', nonce);
	// This can be used for logins after the user has already connected a google account
	// and we need to make sure that the user logged in with the correct account
	if (loginHint) {
		params.append('login_hint', loginHint);
	}
	if (prompt) {
		params.append('prompt', prompt);
	}
	const authUrl = `${url}?${params.toString()}`;
	const responseURL = new URL(
		await Browser.identity.launchWebAuthFlow({
			url: authUrl,
			interactive: true,
		}),
	);
	const responseParams = new URLSearchParams(responseURL.hash.replace('#', ''));
	const jwt = responseParams.get('id_token');
	if (!jwt) {
		throw new Error('JWT is missing');
	}
	return jwt;
}

// TODO: update when we have the final production url
const pinRegistryUrl = 'https://enoki-server-7e33d356b89c.herokuapp.com';

export async function fetchPin(jwt: string): Promise<{ id: string; pin: string }> {
	const response = await fetchWithSentry('fetchUserPin', `${pinRegistryUrl}/get_pin/${jwt}`);
	return response.json();
}

type WalletInputs = {
	jwt: string;
	ephemeralPublicKey: bigint;
	maxEpoch: number;
	jwtRandomness: bigint;
	userPin: bigint;
	keyClaimName?: 'sub' | 'email';
};
type Claim = {
	name: string;
	value_base64: string;
	index_mod_4: number;
};
type ProofPoints = {
	pi_a: string[];
	pi_b: string[][];
	pi_c: string[];
};
export type PartialZkSignature = {
	proof_points: ProofPoints;
	address_seed: string;
	claims: Claim[];
	header_base64: string;
};

// TODO: update when we have the final production url (and a https one)
const zkProofsServerUrl = 'http://185.209.177.123:8000';

export async function createPartialZKSignature({
	jwt,
	ephemeralPublicKey,
	jwtRandomness,
	maxEpoch,
	userPin,
	keyClaimName = 'sub',
}: WalletInputs): Promise<PartialZkSignature> {
	const response = await fetchWithSentry('createZKProofs', `${zkProofsServerUrl}/zkp`, {
		method: 'POST',
		headers: {
			'Content-Type': 'application/json',
		},
		body: JSON.stringify({
			jwt,
			eph_public_key: ephemeralPublicKey.toString(),
			max_epoch: maxEpoch,
			jwt_randomness: jwtRandomness.toString(),
			subject_pin: userPin.toString(),
			key_claim_name: keyClaimName,
		}),
	});
	return response.json();
}
