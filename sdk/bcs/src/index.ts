// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
 * BCS implementation {@see https://github.com/diem/bcs } for JavaScript.
 * Intended to be used for Move applications; supports both NodeJS and browser.
 *
 * For more details and examples {@see README.md }.
 *
 * @module bcs
 * @property {BcsReader}
 */

import { toB58, fromB58 } from './b58.js';
import { toB64, fromB64 } from './b64.js';
import { toHEX, fromHEX } from './hex.js';
import { BcsReader } from './reader.js';
import { BcsWriter, BcsWriterOptions } from './writer.js';
import { bcs } from './bcs.js';
import { encodeStr, decodeStr, splitGenericParameters } from './utils.js';
import { BcsType, BcsTypeOptions, SerializedBcs } from './bcs-type.js';

export * from './legacy-registry.js';

// Re-export all encoding dependencies.
export {
	bcs,
	BcsType,
	type BcsTypeOptions,
	SerializedBcs,
	toB58,
	fromB58,
	toB64,
	fromB64,
	fromHEX,
	toHEX,
	encodeStr,
	decodeStr,
	splitGenericParameters,
	BcsReader,
	BcsWriter,
	type BcsWriterOptions,
};
