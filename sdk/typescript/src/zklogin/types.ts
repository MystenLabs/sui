// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type ProofPoints = {
	a: string[];
	b: string[][];
	c: string[];
};

export type Claim = {
	value: string;
	indexMod4: number;
};

export interface ZkLoginSignatureInputs {
	proofPoints: ProofPoints;
	issBase64Details: Claim;
	headerBase64: string;
	addressSeed: string;
}

export interface ZkLoginSignature {
	inputs: ZkLoginSignatureInputs;
	maxEpoch: number;
	userSignature: string | Uint8Array;
}

export interface ZkLoginDeserializedSignature extends ZkLoginSignature {
	userSignature: Uint8Array;
}
