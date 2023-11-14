// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';

import { bcs } from '../bcs/index.js';
import { Ed25519PublicKey } from '../keypairs/ed25519/publickey.js';
import { Secp256k1PublicKey } from '../keypairs/secp256k1/publickey.js';
import { Secp256r1PublicKey } from '../keypairs/secp256r1/publickey.js';
import { SIGNATURE_SCHEME_TO_FLAG } from './signature-scheme.js';
import type { SignatureScheme } from './signature-scheme.js';
import type { SignaturePubkeyPair } from './utils.js';

/// Decode a multisig signature into a list of signatures, public keys and flags.
export function decodeMultiSig(signature: string): SignaturePubkeyPair[] {
	const parsed = fromB64(signature);
	if (parsed.length < 1 || parsed[0] !== SIGNATURE_SCHEME_TO_FLAG['MultiSig']) {
		throw new Error('Invalid MultiSig flag');
	}

	const multisig = bcs.MultiSig.parse(parsed.slice(1));
	let res: SignaturePubkeyPair[] = new Array(multisig.sigs.length);
	for (let i = 0; i < multisig.sigs.length; i++) {
		let s = multisig.sigs[i];
		let pk_index = as_indices(multisig.bitmap).at(i);
		let pk_bytes = Object.values(multisig.multisig_pk.pk_map[pk_index as number].pubKey)[0];
		const scheme = Object.keys(s)[0] as SignatureScheme;

		if (scheme === 'MultiSig') {
			throw new Error('MultiSig is not supported inside MultiSig');
		}

		if (scheme === 'ZkLogin') {
			throw new Error('ZkLogin is not supported inside MultiSig');
		}

		const SIGNATURE_SCHEME_TO_PUBLIC_KEY = {
			ED25519: Ed25519PublicKey,
			Secp256k1: Secp256k1PublicKey,
			Secp256r1: Secp256r1PublicKey,
		};

		const PublicKey = SIGNATURE_SCHEME_TO_PUBLIC_KEY[scheme];

		res[i] = {
			signatureScheme: scheme,
			signature: Uint8Array.from(Object.values(s)[0]),
			pubKey: new PublicKey(pk_bytes),
			weight: multisig.multisig_pk.pk_map[pk_index as number].weight,
		};
	}
	return res;
}

function as_indices(bitmap: number): Uint8Array {
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
