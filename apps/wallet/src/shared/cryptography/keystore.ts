// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	encrypt as metamaskEncrypt,
	decrypt as metamaskDecrypt,
} from '@metamask/browser-passworder';

type Serializable = string | number | boolean | { [index: string]: Serializable } | Serializable[];

export async function encrypt(password: string, secrets: Serializable): Promise<string> {
	return metamaskEncrypt(password, secrets);
}

export async function decrypt<T extends Serializable>(
	password: string,
	ciphertext: string,
): Promise<T> {
	return (await metamaskDecrypt(password, ciphertext)) as T;
}
