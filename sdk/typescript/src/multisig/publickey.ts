// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';
import { blake2b } from '@noble/hashes/blake2b';
import { bytesToHex } from '@noble/hashes/utils';
import { PublicKey, bytesEqual } from '../cryptography/publickey.js';
import type {
	SerializedSignature,
	SignatureFlag,
	SignatureScheme,
} from '../cryptography/signature.js';
import {
	SIGNATURE_FLAG_TO_SCHEME,
	SIGNATURE_SCHEME_TO_FLAG,
	parseSerializedSignature,
} from '../cryptography/signature.js';
import { normalizeSuiAddress } from '../utils/sui-types.js';
import { builder } from '../builder/bcs.js';
// eslint-disable-next-line import/no-cycle
import { publicKeyFromRawBytes } from '../verify/index.js';

type CompressedSignature =
	| { ED25519: number[] }
	| { Secp256k1: number[] }
	| { Secp256r1: number[] };

type PublicKeyEnum = { ED25519: number[] } | { Secp256k1: number[] } | { Secp256r1: number[] };

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
	) {
		super();

		if (typeof value === 'string') {
			this.rawBytes = fromB64(value);
			this.multisigPublicKey = builder.de('MultiSigPublicKey', this.rawBytes);
		} else if (value instanceof Uint8Array) {
			this.rawBytes = value;
			this.multisigPublicKey = builder.de('MultiSigPublicKey', this.rawBytes);
		} else {
			this.multisigPublicKey = value;
			this.rawBytes = builder.ser('MultiSigPublicKey', value).toBytes();
		}

		this.publicKeys = this.multisigPublicKey.pk_map.map(({ pubKey, weight }) => {
			const [scheme, bytes] = Object.entries(pubKey)[0] as [SignatureScheme, number[]];
			return {
				publicKey: publicKeyFromRawBytes(scheme, Uint8Array.from(bytes)),
				weight,
			};
		});

		if (this.publicKeys.length > MAX_SIGNER_IN_MULTISIG) {
			throw new Error(`Max number of signers in a multisig is ${MAX_SIGNER_IN_MULTISIG}`);
		}
	}

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

	/**
	 * Return the Sui address associated with this MultiSig public key
	 */
	override toSuiAddress(): string {
		// max length = 1 flag byte + (max pk size + max weight size (u8)) * max signer size + 2 threshold bytes (u16)
		const maxLength = 1 + (64 + 1) * MAX_SIGNER_IN_MULTISIG + 2;
		const tmp = new Uint8Array(maxLength);
		tmp.set([SIGNATURE_SCHEME_TO_FLAG['MultiSig']]);

		tmp.set(builder.ser('u16', this.multisigPublicKey.threshold).toBytes(), 1);
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
	async verify(
		message: Uint8Array,
		multisigSignature: Uint8Array | SerializedSignature,
	): Promise<boolean> {
		if (typeof multisigSignature !== 'string') {
			throw new Error('Multisig verification only supports serialized signature');
		}

		const { signatureScheme, multisig } = parseSerializedSignature(multisigSignature);

		if (signatureScheme !== 'MultiSig') {
			throw new Error('Invalid signature scheme');
		}

		let signatureWeight = 0;

		if (
			!bytesEqual(
				builder.ser('MultiSigPublicKey', this.multisigPublicKey).toBytes(),
				builder.ser('MultiSigPublicKey', multisig.multisig_pk).toBytes(),
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

	combinePartialSignatures(signatures: SerializedSignature[]): SerializedSignature {
		let bitmap = 0;
		const compressedSignatures: CompressedSignature[] = new Array(signatures.length);

		for (let i = 0; i < signatures.length; i++) {
			let parsed = parseSerializedSignature(signatures[i]);

			if (parsed.signatureScheme === 'MultiSig') {
				throw new Error('MultiSig is not supported inside MultiSig');
			}

			let bytes = Array.from(parsed.signature.map((x) => Number(x)));

			if (parsed.signatureScheme === 'ED25519') {
				compressedSignatures[i] = { ED25519: bytes };
			} else if (parsed.signatureScheme === 'Secp256k1') {
				compressedSignatures[i] = { Secp256k1: bytes };
			} else if (parsed.signatureScheme === 'Secp256r1') {
				compressedSignatures[i] = { Secp256r1: bytes };
			}

			let publicKeyIndex;
			for (let j = 0; j < this.publicKeys.length; j++) {
				if (bytesEqual(parsed.publicKey, this.publicKeys[j].publicKey.toRawBytes())) {
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

		const bytes = builder.ser('MultiSig', multisig).toBytes();
		let tmp = new Uint8Array(bytes.length + 1);
		tmp.set([SIGNATURE_SCHEME_TO_FLAG['MultiSig']]);
		tmp.set(bytes, 1);
		return toB64(tmp);
	}
}

export function parsePartialSignatures(multisig: MultiSigStruct): ParsedPartialMultiSigSignature[] {
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

		const publicKey = publicKeyFromRawBytes(signatureScheme, pkBytes);

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
