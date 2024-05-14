// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';
import { blake2b } from '@noble/hashes/blake2b';
import { bytesToHex } from '@noble/hashes/utils';

import { bcs } from '../bcs/index.js';
import type { Signer } from '../cryptography/keypair.js';
import { bytesEqual, PublicKey } from '../cryptography/publickey.js';
import {
	SIGNATURE_FLAG_TO_SCHEME,
	SIGNATURE_SCHEME_TO_FLAG,
} from '../cryptography/signature-scheme.js';
import type { SignatureFlag, SignatureScheme } from '../cryptography/signature-scheme.js';
import { parseSerializedSignature } from '../cryptography/signature.js';
import type { SerializedSignature } from '../cryptography/signature.js';
import type { SuiGraphQLClient } from '../graphql/client.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
// eslint-disable-next-line import/no-cycle
import { publicKeyFromRawBytes } from '../verify/index.js';
import { toZkLoginPublicIdentifier } from '../zklogin/publickey.js';
import { MultiSigSigner } from './signer.js';

type CompressedSignature =
	| { ED25519: number[] }
	| { Secp256k1: number[] }
	| { Secp256r1: number[] }
	| { ZkLogin: number[] };

type PublicKeyEnum =
	| { ED25519: number[] }
	| { Secp256k1: number[] }
	| { Secp256r1: number[] }
	| { ZkLogin: number[] };

type PubkeyEnumWeightPair = {
	pubKey: PublicKeyEnum;
	weight: number;
};

type MultiSigPublicKeyStruct = {
	pk_map: PubkeyEnumWeightPair[];
	threshold: number;
};

export type MultiSigStruct = {
	sigs: CompressedSignature[];
	bitmap: number;
	multisig_pk: MultiSigPublicKeyStruct;
};

type ParsedPartialMultiSigSignature = {
	signatureScheme: SignatureScheme;
	signature: Uint8Array;
	publicKey: PublicKey;
	weight: number;
};

export const MAX_SIGNER_IN_MULTISIG = 10;
export const MIN_SIGNER_IN_MULTISIG = 1;
/**
 * A MultiSig public key
 */
export class MultiSigPublicKey extends PublicKey {
	private rawBytes: Uint8Array;
	private multisigPublicKey: MultiSigPublicKeyStruct;
	private publicKeys: {
		weight: number;
		publicKey: PublicKey;
	}[];
	/**
	 * Create a new MultiSigPublicKey object
	 */
	constructor(
		/**
		 *  MultiSig public key as buffer or base-64 encoded string
		 */
		value: string | Uint8Array | MultiSigPublicKeyStruct,
		options: { client?: SuiGraphQLClient } = {},
	) {
		super();

		if (typeof value === 'string') {
			this.rawBytes = fromB64(value);

			this.multisigPublicKey = bcs.MultiSigPublicKey.parse(this.rawBytes);
		} else if (value instanceof Uint8Array) {
			this.rawBytes = value;
			this.multisigPublicKey = bcs.MultiSigPublicKey.parse(this.rawBytes);
		} else {
			this.multisigPublicKey = value;
			this.rawBytes = bcs.MultiSigPublicKey.serialize(value).toBytes();
		}
		if (this.multisigPublicKey.threshold < 1) {
			throw new Error('Invalid threshold');
		}

		const seenPublicKeys = new Set<string>();

		this.publicKeys = this.multisigPublicKey.pk_map.map(({ pubKey, weight }) => {
			const [scheme, bytes] = Object.entries(pubKey)[0] as [SignatureScheme, number[]];
			const publicKeyStr = Uint8Array.from(bytes).toString();

			if (seenPublicKeys.has(publicKeyStr)) {
				throw new Error(`Multisig does not support duplicate public keys`);
			}
			seenPublicKeys.add(publicKeyStr);

			if (weight < 1) {
				throw new Error(`Invalid weight`);
			}

			return {
				publicKey: publicKeyFromRawBytes(scheme, Uint8Array.from(bytes), options),
				weight,
			};
		});

		const totalWeight = this.publicKeys.reduce((sum, { weight }) => sum + weight, 0);

		if (this.multisigPublicKey.threshold > totalWeight) {
			throw new Error(`Unreachable threshold`);
		}

		if (this.publicKeys.length > MAX_SIGNER_IN_MULTISIG) {
			throw new Error(`Max number of signers in a multisig is ${MAX_SIGNER_IN_MULTISIG}`);
		}

		if (this.publicKeys.length < MIN_SIGNER_IN_MULTISIG) {
			throw new Error(`Min number of signers in a multisig is ${MIN_SIGNER_IN_MULTISIG}`);
		}
	}
	/**
	 * 	A static method to create a new MultiSig publickey instance from a set of public keys and their associated weights pairs and threshold.
	 */

	static fromPublicKeys({
		threshold,
		publicKeys,
	}: {
		threshold: number;
		publicKeys: { publicKey: PublicKey; weight: number }[];
	}) {
		return new MultiSigPublicKey({
			pk_map: publicKeys.map(({ publicKey, weight }) => {
				const scheme = SIGNATURE_FLAG_TO_SCHEME[publicKey.flag() as SignatureFlag];

				return {
					pubKey: { [scheme]: Array.from(publicKey.toRawBytes()) } as PublicKeyEnum,
					weight,
				};
			}),
			threshold,
		});
	}

	/**
	 * Checks if two MultiSig public keys are equal
	 */
	override equals(publicKey: MultiSigPublicKey): boolean {
		return super.equals(publicKey);
	}

	/**
	 * Return the byte array representation of the MultiSig public key
	 */
	toRawBytes(): Uint8Array {
		return this.rawBytes;
	}

	getPublicKeys() {
		return this.publicKeys;
	}

	getThreshold() {
		return this.multisigPublicKey.threshold;
	}

	getSigner(...signers: [signer: Signer]) {
		return new MultiSigSigner(this, signers);
	}

	/**
	 * Return the Sui address associated with this MultiSig public key
	 */
	override toSuiAddress(): string {
		// max length = 1 flag byte + (max pk size + max weight size (u8)) * max signer size + 2 threshold bytes (u16)
		const maxLength = 1 + (64 + 1) * MAX_SIGNER_IN_MULTISIG + 2;
		const tmp = new Uint8Array(maxLength);
		tmp.set([SIGNATURE_SCHEME_TO_FLAG['MultiSig']]);

		tmp.set(bcs.u16().serialize(this.multisigPublicKey.threshold).toBytes(), 1);
		// The initial value 3 ensures that following data will be after the flag byte and threshold bytes
		let i = 3;
		for (const { publicKey, weight } of this.publicKeys) {
			const bytes = publicKey.toSuiBytes();
			tmp.set(bytes, i);
			i += bytes.length;
			tmp.set([weight], i++);
		}
		return normalizeSuiAddress(bytesToHex(blake2b(tmp.slice(0, i), { dkLen: 32 })));
	}

	/**
	 * Return the Sui address associated with this MultiSig public key
	 */
	flag(): number {
		return SIGNATURE_SCHEME_TO_FLAG['MultiSig'];
	}

	/**
	 * Verifies that the signature is valid for for the provided message
	 */
	async verify(message: Uint8Array, multisigSignature: SerializedSignature): Promise<boolean> {
		// Multisig verification only supports serialized signature
		const parsed = parseSerializedSignature(multisigSignature);

		if (parsed.signatureScheme !== 'MultiSig') {
			throw new Error('Invalid signature scheme');
		}

		const { multisig } = parsed;

		let signatureWeight = 0;

		if (
			!bytesEqual(
				bcs.MultiSigPublicKey.serialize(this.multisigPublicKey).toBytes(),
				bcs.MultiSigPublicKey.serialize(multisig.multisig_pk).toBytes(),
			)
		) {
			return false;
		}

		for (const { publicKey, weight, signature } of parsePartialSignatures(multisig)) {
			if (!(await publicKey.verify(message, signature))) {
				return false;
			}

			signatureWeight += weight;
		}

		return signatureWeight >= this.multisigPublicKey.threshold;
	}

	/**
	 * Combines multiple partial signatures into a single multisig, ensuring that each public key signs only once
	 * and that all the public keys involved are known and valid, and then serializes multisig into the standard format
	 */
	combinePartialSignatures(signatures: SerializedSignature[]): SerializedSignature {
		if (signatures.length > MAX_SIGNER_IN_MULTISIG) {
			throw new Error(`Max number of signatures in a multisig is ${MAX_SIGNER_IN_MULTISIG}`);
		}

		let bitmap = 0;
		const compressedSignatures: CompressedSignature[] = new Array(signatures.length);

		for (let i = 0; i < signatures.length; i++) {
			let parsed = parseSerializedSignature(signatures[i]);
			if (parsed.signatureScheme === 'MultiSig') {
				throw new Error('MultiSig is not supported inside MultiSig');
			}

			let publicKey;
			if (parsed.signatureScheme === 'ZkLogin') {
				publicKey = toZkLoginPublicIdentifier(
					parsed.zkLogin?.addressSeed,
					parsed.zkLogin?.iss,
				).toRawBytes();
			} else {
				publicKey = parsed.publicKey;
			}

			compressedSignatures[i] = {
				[parsed.signatureScheme]: Array.from(parsed.signature.map((x: number) => Number(x))),
			} as CompressedSignature;

			let publicKeyIndex;
			for (let j = 0; j < this.publicKeys.length; j++) {
				if (bytesEqual(publicKey, this.publicKeys[j].publicKey.toRawBytes())) {
					if (bitmap & (1 << j)) {
						throw new Error('Received multiple signatures from the same public key');
					}

					publicKeyIndex = j;
					break;
				}
			}

			if (publicKeyIndex === undefined) {
				throw new Error('Received signature from unknown public key');
			}

			bitmap |= 1 << publicKeyIndex;
		}

		let multisig: MultiSigStruct = {
			sigs: compressedSignatures,
			bitmap,
			multisig_pk: this.multisigPublicKey,
		};
		const bytes = bcs.MultiSig.serialize(multisig, { maxSize: 8192 }).toBytes();
		let tmp = new Uint8Array(bytes.length + 1);
		tmp.set([SIGNATURE_SCHEME_TO_FLAG['MultiSig']]);
		tmp.set(bytes, 1);
		return toB64(tmp);
	}
}

/**
 * Parse multisig structure into an array of individual signatures: signature scheme, the actual individual signature, public key and its weight.
 */
export function parsePartialSignatures(
	multisig: MultiSigStruct,
	options: { client?: SuiGraphQLClient } = {},
): ParsedPartialMultiSigSignature[] {
	let res: ParsedPartialMultiSigSignature[] = new Array(multisig.sigs.length);
	for (let i = 0; i < multisig.sigs.length; i++) {
		const [signatureScheme, signature] = Object.entries(multisig.sigs[i])[0] as [
			SignatureScheme,
			number[],
		];
		const pkIndex = asIndices(multisig.bitmap).at(i)!;
		const pair = multisig.multisig_pk.pk_map[pkIndex];
		const pkBytes = Uint8Array.from(Object.values(pair.pubKey)[0]);

		if (signatureScheme === 'MultiSig') {
			throw new Error('MultiSig is not supported inside MultiSig');
		}

		const publicKey = publicKeyFromRawBytes(signatureScheme, pkBytes, options);

		res[i] = {
			signatureScheme,
			signature: Uint8Array.from(signature),
			publicKey: publicKey,
			weight: pair.weight,
		};
	}
	return res;
}

function asIndices(bitmap: number): Uint8Array {
	if (bitmap < 0 || bitmap > 1024) {
		throw new Error('Invalid bitmap');
	}
	let res: number[] = [];
	for (let i = 0; i < 10; i++) {
		if ((bitmap & (1 << i)) !== 0) {
			res.push(i);
		}
	}
	return Uint8Array.from(res);
}
