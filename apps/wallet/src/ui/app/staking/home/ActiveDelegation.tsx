// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { DELEGATION_OBJECT_TYPE, type Delegation } from '@mysten/sui.js';
import { useMemo } from 'react';

import { suiObjectsAdapterSelectors } from '../../redux/slices/sui-objects';
import { getValidatorSelector } from '../selectors';
import { DelegationCard, DelegationState } from './DelegationCard';
import { useAppSelector } from '_hooks';

import type { RootState } from '_redux/RootReducer';

interface Props {
    id: string;
}

export function ActiveDelegation({ id }: Props) {
    const delegationSelector = useMemo(
        () => (state: RootState) => {
            const suiObj = suiObjectsAdapterSelectors.selectById(state, id);
            if (
                suiObj &&
                'type' in suiObj.data &&
                suiObj.data.type === DELEGATION_OBJECT_TYPE
            ) {
                return suiObj.data.fields as Delegation;
            }
            return undefined;
        },
        [id]
    );

    const delegation = useAppSelector(delegationSelector);
    const validatorAddress = delegation?.validator_address;
    const validatorSelector = useMemo(
        () => getValidatorSelector(validatorAddress),
        [validatorAddress]
    );
    const validator = useAppSelector(validatorSelector);
    const validatorName = useMemo(() => {
        if (!validator) {
            return null;
        }
        return Buffer.from(validator.fields.name, 'base64').toString();
    }, [validator]);

    if (!validator || !delegation || !validatorName) {
        return null;
    }

    return (
        <DelegationCard
            name={validatorName}
            staked={delegation.principal_sui_amount}
            state={DelegationState.EARNING}
        />
    );
}
