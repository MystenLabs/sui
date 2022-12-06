// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject, isSuiObject, type SuiSystemState } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';
import { useMemo } from 'react';

import { notEmpty } from '../helpers';
import { useAppSelector } from '../hooks';
import { api } from '../redux/store/thunk-extras';

const STATE_OBJECT = '0x5';

interface PendingDelegation {
    name: string;
    staked: bigint;
}

/**
 * Fetches the pending delegations from the system object. This is currently pretty hacky and expensive.
 */
export function usePendingDelegation(): [PendingDelegation[], UseQueryResult] {
    const address = useAppSelector(({ account: { address } }) => address);

    // TODO: Use generlized `useGetObject` hook when it lands:
    const objectQuery = useQuery(['object', STATE_OBJECT], async () => {
        return api.instance.fullNode.getObject(STATE_OBJECT);
    });

    const { data } = objectQuery;

    const pendingDelegation = useMemo(() => {
        if (
            !address ||
            !data ||
            !isSuiObject(data.details) ||
            !isSuiMoveObject(data.details.data)
        ) {
            return [];
        }

        const systemState = data.details.data.fields as SuiSystemState;

        const pendingDelegationsPerValidator =
            systemState.validators.fields.active_validators
                .map((validator) => {
                    const pendingDelegations =
                        validator.fields.delegation_staking_pool.fields
                            .pending_delegations;

                    if (!Array.isArray(pendingDelegations)) return null;

                    const filteredDelegations = pendingDelegations.filter(
                        (delegation) => delegation.fields.delegator === address
                    );

                    if (!filteredDelegations.length) return null;

                    // TODO: Follow-up about why this is base64 encoded:
                    const name = Buffer.from(
                        validator.fields.metadata.fields.name,
                        'base64'
                    ).toString();

                    return {
                        name,
                        staked: filteredDelegations.reduce(
                            (acc, delegation) =>
                                acc + BigInt(delegation.fields.sui_amount),
                            0n
                        ),
                    } as PendingDelegation;
                })
                .filter(notEmpty);

        return pendingDelegationsPerValidator;
    }, [data, address]);

    return [pendingDelegation, objectQuery];
}
