// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiEventEnvelope } from '@mysten/sui.js';

export function getValidatorMoveEvent(
    validatorsEvent: SuiEventEnvelope[],
    validatorAddress: string
) {
    const event = validatorsEvent.find(({ event }) => {
        if (event.type === 'moveEvent') {
            const { content } = event;
            return content.fields.validator_address === validatorAddress;
        }
        return false;
    });

    return event && event.event.type === 'moveEvent' ? event.event.content : null;
}
