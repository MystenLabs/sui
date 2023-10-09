// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from 'vitest';

import { getInnerType } from '../../src';

describe('Helper functions should return correct data', () => {
	it('Should parse inner types properly', () => {
		const extensionType =
			'0x2::kiosk_extension::ExtensionKey<0x3::hero::Hero<0x5::hero::Awesome<0x6::hero::Level>>>';

		expect(getInnerType(extensionType)).toEqual(
			'0x3::hero::Hero<0x5::hero::Awesome<0x6::hero::Level>>',
		);

		expect(getInnerType(extensionType, 1)).toEqual('0x5::hero::Awesome<0x6::hero::Level>');

		expect(getInnerType(extensionType, 2)).toEqual('0x6::hero::Level');

		expect(getInnerType(extensionType, 9)).toEqual('');
	});
});
