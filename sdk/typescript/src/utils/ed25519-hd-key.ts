// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This is adapted from https://github.com/alepop/ed25519-hd-key replacing create-hmac
// with @noble/hashes to be browser compatible.

import { sha512 } from '@noble/hashes/sha512';
import { hmac } from '@noble/hashes/hmac';
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
  const I = h.update(Buffer.from(seed, 'hex')).digest();
  const IL = I.slice(0, 32);
  const IR = I.slice(32);
  return {
    key: IL,
    chainCode: IR,
  };
};

export const CKDPriv = ({ key, chainCode }: Keys, index: number): Keys => {
  const indexBuffer = Buffer.allocUnsafe(4);
  indexBuffer.writeUInt32BE(index, 0);

  const data = Buffer.concat([Buffer.alloc(1, 0), key, indexBuffer]);

  const I = hmac.create(sha512, chainCode).update(data).digest();
  const IL = I.slice(0, 32);
  const IR = I.slice(32);
  return {
    key: IL,
    chainCode: IR,
  };
};

export const getPublicKey = (
  privateKey: Uint8Array,
  withZeroByte = true
): Uint8Array => {
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

export const derivePath = (
  path: Path,
  seed: Hex,
  offset = HARDENED_OFFSET
): Keys => {
  if (!isValidPath(path)) {
    throw new Error('Invalid derivation path');
  }

  const { key, chainCode } = getMasterKeyFromSeed(seed);
  const segments = path
    .split('/')
    .slice(1)
    .map(replaceDerive)
    .map((el) => parseInt(el, 10));

  return segments.reduce(
    (parentKeys, segment) => CKDPriv(parentKeys, segment + offset),
    { key, chainCode }
  );
};
