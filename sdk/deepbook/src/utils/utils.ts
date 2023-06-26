// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PoolInfo, Records } from './dto';

export function getPoolInfoByRecords(
	tokenType1: string,
	tokenType2: string,
	records: Records,
): PoolInfo {
	for (const ele of records.pools) {
		if (
			ele.type.indexOf(tokenType1) !== -1 &&
			ele.type.indexOf(tokenType2) !== -1 &&
			ele.type.indexOf(tokenType1) < ele.type.indexOf(tokenType2)
		) {
			return {
				needChange: false,
				clob: String(ele.clob),
				type: String(ele.type),
				tickSize: ele.tickSize,
			};
		} else if (
			ele.type.indexOf(tokenType1) !== -1 &&
			ele.type.indexOf(tokenType2) !== -1 &&
			ele.type.indexOf(tokenType1) > ele.type.indexOf(tokenType2)
		) {
			return {
				needChange: true,
				clob: String(ele.clob),
				type: String(ele.type),
				tickSize: ele.tickSize,
			};
		}
	}
	throw new Error('Pool not found');
}
