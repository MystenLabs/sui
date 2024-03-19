// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This is adapted from https://github.com/alepop/ed25519-hd-key replacing create-hmac
// with @noble/hashes to be browser compatible.

import { fromHEX } from '@mysten/bcs';
import { hmac } from '@noble/hashes/hmac';
import { sha512 } from '@noble/hashes/sha512';
import nacl from 'tweetnacl';

type Hex = string;
type Path = string;

type Keys = {
	key: Uint8Array;
	chainCode: Uint8Array;
};

const ED25519_CURVE = 'ed25519 seed';
const HARDENED_OFFSET = 0x80000000;

export const pathRegex = new RegExp("^m(\\/[0-9]+')+$");

export const replaceDerive = (val: string): string => val.replace("'", '');

export const getMasterKeyFromSeed = (seed: Hex): Keys => {
	const h = hmac.create(sha512, ED25519_CURVE);
	const I = h.update(fromHEX(seed)).digest();
	const IL = I.slice(0, 32);
	const IR = I.slice(32);
	return {
		key: IL,
		chainCode: IR,
	};
};

const CKDPriv = ({ key, chainCode }: Keys, index: number): Keys => {
	const indexBuffer = new ArrayBuffer(4);
	const cv = new DataView(indexBuffer);
	cv.setUint32(0, index);

	const data = new Uint8Array(1 + key.length + indexBuffer.byteLength);
	data.set(new Uint8Array(1).fill(0));
	data.set(key, 1);
	data.set(new Uint8Array(indexBuffer, 0, indexBuffer.byteLength), key.length + 1);

	const I = hmac.create(sha512, chainCode).update(data).digest();
	const IL = I.slice(0, 32);
	const IR = I.slice(32);
	return {
		key: IL,
		chainCode: IR,
	};
};

export const getPublicKey = (privateKey: Uint8Array, withZeroByte = true): Uint8Array => {
	const keyPair = nacl.sign.keyPair.fromSeed(privateKey);
	const signPk = keyPair.secretKey.subarray(32);
	const newArr = new Uint8Array(signPk.length + 1);
	newArr.set([0]);
	newArr.set(signPk, 1);
	return withZeroByte ? newArr : signPk;
};

export const isValidPath = (path: string): boolean => {
	if (!pathRegex.test(path)) {
		return false;
	}
	return !path
		.split('/')
		.slice(1)
		.map(replaceDerive)
		.some(isNaN as any /* ts T_T*/);
};

export const derivePath = (path: Path, seed: Hex, offset = HARDENED_OFFSET): Keys => {
	if (!isValidPath(path)) {
		throw new Error('Invalid derivation path');
	}

	const { key, chainCode } = getMasterKeyFromSeed(seed);
	const segments = path
		.split('/')
		.slice(1)
		.map(replaceDerive)
		.map((el) => parseInt(el, 10));

	return segments.reduce((parentKeys, segment) => CKDPriv(parentKeys, segment + offset), {
		key,
		chainCode,
	});
};
