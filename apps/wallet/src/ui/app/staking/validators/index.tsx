// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiObject, isSuiMoveObject } from '@mysten/sui.js';
import { useState, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';

import { getName, STATE_OBJECT } from '../usePendingDelegation';
import { SelectValidatorCard } from './SelectValidatorCard';
import { ValidatorsCard } from './ValidatorsCard';
import {
    activeDelegationIDsSelector,
    totalActiveStakedSelector,
} from '_app/staking/selectors';
import Alert from '_components/alert';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useAppSelector, useObjectsState, useGetObject } from '_hooks';

import type { ValidatorState } from '../ValidatorDataTypes';

export type ValidatorsProp = {
    name: string;
    apy: number | string;
    logo: string | null;
    address: string;
    pendingDelegationAmount: bigint;
};

export function Validators() {
    const [showModal, setShowModal] = useState(true);

    const navigate = useNavigate();
    const close = () => {
        navigate('/');
    };

    const { loading, error } = useObjectsState();
    const activeDelegationIDs = useAppSelector(activeDelegationIDsSelector);
    const totalStaked = useAppSelector(totalActiveStakedSelector);
    const accountAddress = useAppSelector(({ account }) => account.address);
    const { data, isLoading } = useGetObject(STATE_OBJECT);

    const validatorsData =
        data && isSuiObject(data.details) && isSuiMoveObject(data.details.data)
            ? (data.details.data.fields as ValidatorState)
            : null;

    const validators = useMemo(() => {
        if (!validatorsData) return [];
        return validatorsData.validators.fields.active_validators
            .map((av) => {
                const rawName = av.fields.metadata.fields.name;
                const {
                    sui_balance,
                    starting_epoch,
                    pending_delegations,
                    delegation_token_supply,
                } = av.fields.delegation_staking_pool.fields;

                const num_epochs_participated =
                    validatorsData.epoch - starting_epoch;

                const APY = Math.pow(
                    1 +
                        (sui_balance - delegation_token_supply.fields.value) /
                            delegation_token_supply.fields.value,
                    365 / num_epochs_participated - 1
                );

                const pending_delegationsByAddress = pending_delegations
                    ? pending_delegations.filter(
                          (d) => d.fields.delegator === accountAddress
                      )
                    : [];

                return {
                    name: getName(rawName),
                    apy: APY > 0 ? APY : 'N/A',
                    logo: null,
                    address: av.fields.metadata.fields.sui_address,
                    pendingDelegationAmount:
                        pending_delegationsByAddress.reduce(
                            (acc, fields) =>
                                (acc += BigInt(fields.fields.sui_amount || 0n)),
                            0n
                        ),
                };
            })
            .sort((a, b) => (a.name > b.name ? 1 : -1));
    }, [accountAddress, validatorsData]);

    // TODO - get this from the metadata
    const earnedRewards = BigInt(0);

    const totalStakedIncludingPending =
        totalStaked +
        validators.reduce(
            (acc, { pendingDelegationAmount }) => acc + pendingDelegationAmount,
            0n
        );

    const hasDelegations = Boolean(totalStakedIncludingPending);

    const pageTitle = hasDelegations
        ? 'Stake & Earn SUI'
        : 'Select a Validator';

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={isLoading ? 'Loading' : pageTitle}
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            <div className="w-full flex flex-col flex-nowrap h-full overflow-x-scroll">
                <Loading
                    loading={isLoading || loading}
                    className="flex justify-center w-full items-center h-full"
                >
                    {error ? (
                        <Alert className="mb-2">
                            <strong>
                                Sync error (data might be outdated).
                            </strong>{' '}
                            <small>{error.message}</small>
                        </Alert>
                    ) : null}

                    {hasDelegations ? (
                        <ValidatorsCard
                            validators={validators}
                            earnedRewards={earnedRewards}
                            activeDelegationIDs={activeDelegationIDs}
                            totalStaked={totalStakedIncludingPending}
                        />
                    ) : (
                        <SelectValidatorCard />
                    )}
                </Loading>
            </div>
        </Overlay>
    );
}
