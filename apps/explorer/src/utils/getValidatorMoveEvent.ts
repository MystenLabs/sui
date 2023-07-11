// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiEvent } from '@mysten/sui.js';

export function getValidatorMoveEvent(validatorsEvent: SuiEvent[], validatorAddress: string) {
	const event = validatorsEvent.find(
		({ parsedJson }) => parsedJson!.validator_address === validatorAddress,
	);

	return event && event.parsedJson;
}
