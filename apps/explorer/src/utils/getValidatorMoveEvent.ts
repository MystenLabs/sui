// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getMoveEvent,
    isEventType,
    type SuiEventEnvelope,
} from '@mysten/sui.js';

export function getValidatorMoveEvent(
    validatorsEvent: SuiEventEnvelope[],
    validatorAddress: string
) {
    const event = validatorsEvent.find(({ event }) => {
        if (isEventType(event, 'moveEvent')) {
            const moveEvent = getMoveEvent(event)!;
            return moveEvent.fields.validator_address === validatorAddress;
        }
        return false;
    });

    return event && isEventType(event.event, 'moveEvent')
        ? getMoveEvent(event.event)
        : null;
}
