// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';

import { SIGNATURE_SCHEME_TO_FLAG } from '../cryptography/signature-scheme.js';
import { zkLoginBcs } from './bcs.js';
import type { ZkLoginDeserializedSignature, ZkLoginSignature } from './types.js';

function getZkLoginSignatureBytes({ inputs, maxEpoch, userSignature }: ZkLoginSignature) {
	return zkLoginBcs
		.ser(
			'ZkLoginSignature',
			{
				inputs,
				maxEpoch,
				userSignature: typeof userSignature === 'string' ? fromB64(userSignature) : userSignature,
			},
			{ maxSize: 2048 },
		)
		.toBytes();
}

export function getZkLoginSignature({ inputs, maxEpoch, userSignature }: ZkLoginSignature) {
	const bytes = getZkLoginSignatureBytes({ inputs, maxEpoch, userSignature });
	const signatureBytes = new Uint8Array(bytes.length + 1);
	signatureBytes.set([SIGNATURE_SCHEME_TO_FLAG.ZkLogin]);
	signatureBytes.set(bytes, 1);
	return toB64(signatureBytes);
}

export function parseZkLoginSignature(
	signature: string | Uint8Array,
): ZkLoginDeserializedSignature {
	return zkLoginBcs.de('ZkLoginSignature', signature);
}
