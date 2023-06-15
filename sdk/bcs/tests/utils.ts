// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { BCS } from '../src/index';

/** Serialize and deserialize the result. */
export function serde(bcs: BCS, type: any, data: any): any {
	let ser = bcs.ser(type, data).toString('hex');
	let de = bcs.de(type, ser, 'hex');
	return de;
}
