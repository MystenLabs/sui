// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { isSuiObject, isSuiMoveObject, SUI_TYPE_ARG } from '@mysten/sui.js';
import { useState, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';

import { FEATURES } from '../../experimentation/features';
import { SelectValidatorCard } from '../stake/SelectValidatorCard';
import { getName, STATE_OBJECT } from '../usePendingDelegation';
import { ActiveDelegation } from './ActiveDelegation';
import { DelegationCard, DelegationState } from './DelegationCard';
import StakeAmount from './StakeAmount';
import BottomMenuLayout, {
    Menu,
    Content,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Card, CardItem } from '_app/shared/card';
import { Text } from '_app/shared/text';
import {
    activeDelegationIDsSelector,
    totalActiveStakedSelector,
} from '_app/staking/selectors';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useAppSelector, useObjectsState, useGetObject } from '_hooks';

import type { ValidatorState } from '../ValidatorDataTypes';

type ValidatorProp = {
    name: string;
    apy: number | string;
    logo: string | null;
    address: string;
    pendingDelegationAmount: bigint;
};

function StakeHome() {
    const { loading, error, showError } = useObjectsState();

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

    const totalStakedIncludingPending =
        totalStaked +
        validators.reduce(
            (acc, { pendingDelegationAmount }) => acc + pendingDelegationAmount,
            0n
        );

    const hasDelegations =
        activeDelegationIDs.length > 0 || validators.length > 0;
    const [showModal, setShowModal] = useState(true);

    const navigate = useNavigate();
    const close = () => {
        navigate('/tokens');
    };

    const pageTitle = hasDelegations
        ? 'Stake & Earn SUI'
        : 'Select a Validator';

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={!loading && !isLoading && pageTitle}
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            <div className="w-full flex flex-col flex-nowrap h-full overflow-x-scroll">
                {showError && error ? (
                    <Alert className="mb-2">
                        <strong>Sync error (data might be outdated).</strong>{' '}
                        <small>{error.message}</small>
                    </Alert>
                ) : null}
                <Loading
                    loading={loading || isLoading}
                    className="flex h-full items-center justify-center"
                >
                    {hasDelegations ? (
                        <StakedHomeCard
                            activeDelegationIDs={activeDelegationIDs}
                            validators={validators}
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

type StakedCardProps = {
    activeDelegationIDs: string[];
    validators: ValidatorProp[];
    totalStaked: bigint;
};

function StakedHomeCard({
    activeDelegationIDs,
    validators,
    totalStaked,
}: StakedCardProps) {
    const hasDelegations =
        activeDelegationIDs.length > 0 || validators.length > 0;

    const numOfValidators = activeDelegationIDs.length + validators.length;

    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

    return (
        <div className="flex flex-col flex-nowrap h-full overflow-x-scroll w-full">
            <BottomMenuLayout>
                <Content>
                    <div className="mb-4">
                        <Card
                            padding="none"
                            header={
                                <div className="py-2.5">
                                    <Text
                                        variant="captionSmall"
                                        weight="semibold"
                                        color="steel-darker"
                                    >
                                        STAKING ON {numOfValidators}
                                        {numOfValidators > 1
                                            ? ' VALIDATORS'
                                            : ' VALIDATOR'}
                                    </Text>
                                </div>
                            }
                        >
                            <div className="flex divide-x divide-solid divide-gray-45 divide-y-0">
                                <CardItem
                                    title="Your Stake"
                                    value={
                                        <StakeAmount
                                            balance={totalStaked}
                                            type={SUI_TYPE_ARG}
                                            diffSymbol
                                            size="heading4"
                                            color="gray-90"
                                            symbolColor="steel"
                                        />
                                    }
                                />
                                {/* TODO: show the actual Rewards Collected value https://github.com/MystenLabs/sui/issues/3605 */}
                                <CardItem
                                    title="EARNED"
                                    value={
                                        <StakeAmount
                                            balance={BigInt(0)}
                                            type={SUI_TYPE_ARG}
                                            diffSymbol
                                            symbolColor="gray-60"
                                            size="heading4"
                                            color="gray-60"
                                        />
                                    }
                                />
                            </div>
                        </Card>

                        <div className="grid grid-cols-2 gap-2.5 mt-4">
                            {hasDelegations ? (
                                <>
                                    {validators
                                        .filter(
                                            ({ pendingDelegationAmount }) =>
                                                pendingDelegationAmount > 0
                                        )
                                        .map(
                                            (
                                                {
                                                    name,
                                                    pendingDelegationAmount,
                                                    address,
                                                },
                                                index
                                            ) => (
                                                <DelegationCard
                                                    key={index}
                                                    name={name}
                                                    staked={
                                                        pendingDelegationAmount
                                                    }
                                                    state={
                                                        DelegationState.WARM_UP
                                                    }
                                                    address={address}
                                                />
                                            )
                                        )}

                                    {activeDelegationIDs.map((delegationID) => (
                                        <ActiveDelegation
                                            key={delegationID}
                                            id={delegationID}
                                        />
                                    ))}
                                </>
                            ) : (
                                <div className="flex mt-7.5 items-center justify-center grid-cols-2 text-bodySmall">
                                    <Text
                                        variant="caption"
                                        weight="semibold"
                                        color="gray-75"
                                    >
                                        No active stakes found
                                    </Text>
                                </div>
                            )}
                        </div>
                    </div>
                </Content>
                <Menu stuckClass="staked-cta" className="w-full px-0 pb-0 mx-0">
                    <Button
                        size="large"
                        mode="neutral"
                        href="new"
                        disabled={!stakingEnabled}
                        className="!text-steel-darker w-full"
                    >
                        <Icon
                            icon={SuiIcons.Plus}
                            className="text-body text-gray-65 font-normal"
                        />
                        Stake SUI
                    </Button>
                </Menu>
            </BottomMenuLayout>
        </div>
    );
}

export default StakeHome;
