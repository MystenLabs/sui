// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS, fromB64, toB64 } from '@mysten/bcs';
import { SIGNATURE_SCHEME_TO_FLAG } from '@mysten/sui.js/cryptography';
import { bcs } from '@mysten/sui.js/bcs';

export const zkBcs = new BCS(bcs);

type ProofPoints = {
	a: string[];
	b: string[][];
	c: string[];
};

type IssBase64 = {
	value: string;
	indexMod4: number;
};

export interface ZkSignatureInputs {
	proofPoints: ProofPoints;
	issBase64Details: IssBase64;
	headerBase64: string;
	addressSeed: string;
}

export interface ZkSignature {
	inputs: ZkSignatureInputs;
	maxEpoch: number;
	userSignature: string | Uint8Array;
}

zkBcs.registerStructType('ZkSignature', {
	inputs: {
		proof_points: {
			a: [BCS.VECTOR, BCS.STRING],
			b: [BCS.VECTOR, [BCS.VECTOR, BCS.STRING]],
			c: [BCS.VECTOR, BCS.STRING],
		},
		iss_base64_details: {
			value: BCS.STRING,
			index_mod_4: BCS.U8,
		},
		header_base64: BCS.STRING,
		address_seed: BCS.STRING,
	},
	max_epoch: BCS.U64,
	user_signature: [BCS.VECTOR, BCS.U8],
});

function getZkSignatureBytes({ inputs, maxEpoch, userSignature }: ZkSignature) {
	return zkBcs
		.ser(
			'ZkSignature',
			{
				inputs: {
					proof_points: inputs.proofPoints,
					iss_base64_details: {
						value: inputs.issBase64Details.value,
						index_mod_4: inputs.issBase64Details.indexMod4,
					},
					header_base64: inputs.headerBase64,
					address_seed: inputs.addressSeed,
				},
				max_epoch: maxEpoch,
				user_signature: typeof userSignature === 'string' ? fromB64(userSignature) : userSignature,
			},
			{ maxSize: 2048 },
		)
		.toBytes();
}

export function getZkSignature({ inputs, maxEpoch, userSignature }: ZkSignature) {
	const bytes = getZkSignatureBytes({ inputs, maxEpoch, userSignature });
	const signatureBytes = new Uint8Array(bytes.length + 1);
	signatureBytes.set([SIGNATURE_SCHEME_TO_FLAG['Zk']]);
	signatureBytes.set(bytes, 1);
	return toB64(signatureBytes);
}
