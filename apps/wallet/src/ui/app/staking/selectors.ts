// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject, DELEGATION_OBJECT_TYPE } from '@mysten/sui.js';
import { createSelector } from '@reduxjs/toolkit';

import { ownedObjects } from '_redux/slices/account';
import { suiSystemObjectSelector } from '_redux/slices/sui-objects';

import type { SuiMoveObjectTyped, Delegation } from '@mysten/sui.js';

export const delegationsSelector = createSelector(
    ownedObjects,
    (objects) =>
        objects
            .map((obj) => obj.data)
            .filter(
                (obj) => 'type' in obj && obj.type === DELEGATION_OBJECT_TYPE
            ) as unknown as SuiMoveObjectTyped<Delegation>[]
);

export const activeDelegationIDsSelector = createSelector(
    delegationsSelector,
    (delegations) => delegations.map(({ fields }) => fields.id)
);

export const totalActiveStakedSelector = createSelector(
    delegationsSelector,
    (activeDelegations) =>
        activeDelegations.reduce((total, obj) => {
            total += BigInt(obj.fields.principal_sui_amount);
            return total;
        }, BigInt(0))
);

export const epochSelector = createSelector(
    suiSystemObjectSelector,
    (systemObj) =>
        systemObj && isSuiMoveObject(systemObj.data)
            ? (systemObj.data.fields.epoch as number)
            : null
);

export function getValidatorSelector(validatorAddress?: string) {
    // TODO this is limited only to the active and next set of validators. Is there a way to access the list of all validators?
    return createSelector(suiSystemObjectSelector, (systemObj) => {
        const { data } = systemObj || {};
        if (isSuiMoveObject(data)) {
            const { active_validators: active, next_epoch_validators: next } =
                data.fields.validators.fields;
            const validator: SuiMoveObject | undefined = [
                ...active.map((v: SuiMoveObject) => v.fields.metadata),
                ...next,
            ].find(
                (aValidator) =>
                    aValidator.fields.sui_address === validatorAddress
            );
            return validator;
        }
        return undefined;
    });
}
