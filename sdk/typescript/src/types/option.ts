// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type Option<T> =
	| T
	| {
			fields: {
				vec: '';
			};
			type: string;
	  };

export function getOption<T>(option: Option<T>): T | undefined {
	if (
		typeof option === 'object' &&
		option !== null &&
		'type' in option &&
		option.type.startsWith('0x1::option::Option<')
	) {
		return undefined;
	}
	return option as T;
}
