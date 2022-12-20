// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject, type SuiAddress } from '@mysten/sui.js';
import { useQuery, type UseQueryResult } from '@tanstack/react-query';
import { useMemo } from 'react';

import { notEmpty } from '_helpers';
import { useGetObject, useAppSelector } from '_hooks';

export const STATE_OBJECT = '0x5';
export const VALDIATOR_NAME = /^[A-Z-_.\s0-9]+$/i;

// TODO: Generalize into SDK:
interface SystemStateObject {
    validators: {
        fields: {
            active_validators: {
                fields: {
                    metadata: {
                        fields: {
                            name: string | number[];
                        };
                    };
                    delegation_staking_pool: {
                        fields: {
                            validator_address: SuiAddress;
                            // TODO: Figure out why this is an empty string sometimes:
                            pending_delegations:
                                | string
                                | {
                                      fields: {
                                          delegator: SuiAddress;
                                          sui_amount: number;
                                      };
                                  }[];
                        };
                    };
                };
            }[];
        };
    };
}

export function getName(rawName: string | number[]) {
    let name: string;

    if (Array.isArray(rawName)) {
        name = String.fromCharCode(...rawName);
    } else {
        name = Buffer.from(rawName, 'base64').toString();
        if (!VALDIATOR_NAME.test(name)) {
            name = rawName;
        }
    }
    return name;
}
interface PendingDelegation {
    name: string;
    staked: bigint;
    validatorAddress: SuiAddress;
}

/**
 * Fetches the pending delegations from the system object. This is currently pretty hacky and expensive.
 */
export function usePendingDelegation(): [PendingDelegation[], UseQueryResult] {
    const address = useAppSelector(({ account: { address } }) => address);

    const objectQuery = useGetObject(STATE_OBJECT);

    const { data } = objectQuery;

    const pendingDelegation = useMemo(() => {
        if (
            !address ||
            !data ||
            !is(data.details, SuiObject) ||
            data.details.data.dataType !== 'moveObject'
        ) {
            return [];
        }

        const systemState = data.details.data.fields as SystemStateObject;

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

                    return {
                        name: getName(validator.fields.metadata.fields.name),
                        validatorAddress:
                            validator.fields.delegation_staking_pool.fields
                                .validator_address,
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
