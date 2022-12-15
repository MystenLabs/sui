// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

import { FEATURES } from '../../experimentation/features';
import { usePendingDelegation } from '../usePendingDelegation';
import { ActiveDelegation } from './ActiveDelegation';
import { DelegationCard, DelegationState } from './DelegationCard';
import BottomMenuLayout, {
    Menu,
    Content,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Card, CardItem } from '_app/shared/card';
import CoinBalance from '_app/shared/coin-balance';
import { Text } from '_app/shared/text';
import {
    activeDelegationIDsSelector,
    totalActiveStakedSelector,
} from '_app/staking/selectors';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useAppSelector, useObjectsState } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

function StakeHome() {
    const { loading, error, showError } = useObjectsState();

    const activeDelegationIDs = useAppSelector(activeDelegationIDsSelector);

    const [pendingDelegations, { isLoading: pendingDelegationsLoading }] =
        usePendingDelegation();

    const hasDelegations =
        activeDelegationIDs.length > 0 || pendingDelegations.length > 0;
    const [showModal, setShowModal] = useState(true);

    const navigate = useNavigate();
    const close = useCallback(() => {
        navigate('/tokens');
    }, [navigate]);

    const pageTitle = hasDelegations
        ? 'Stake & Earn SUI'
        : 'Select a Validator';

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={!loading && !pendingDelegationsLoading && pageTitle}
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
                    loading={loading || pendingDelegationsLoading}
                    className="flex h-full items-center justify-center"
                >
                    {hasDelegations ? (
                        <StakedHomeCard
                            activeDelegationIDs={activeDelegationIDs}
                            pendingDelegations={pendingDelegations}
                        />
                    ) : (
                        '<ActiveValidatorsCard />'
                    )}
                </Loading>
            </div>
        </Overlay>
    );
}

type StakedCardProps = {
    activeDelegationIDs: string[];
    pendingDelegations: {
        name: string;
        staked: bigint;
        validatorAddress: string;
    }[];
};

function StakedHomeCard({
    activeDelegationIDs,
    pendingDelegations,
}: StakedCardProps) {
    const totalStaked = useAppSelector(totalActiveStakedSelector);
    const totalStakedIncludingPending =
        totalStaked +
        pendingDelegations.reduce((acc, { staked }) => acc + staked, 0n);

    const hasDelegations =
        activeDelegationIDs.length > 0 || pendingDelegations.length > 0;

    const numOfValidators =
        activeDelegationIDs.length + pendingDelegations.length;

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
                                        <CoinBalance
                                            balance={
                                                totalStakedIncludingPending
                                            }
                                            type={GAS_TYPE_ARG}
                                            diffSymbol
                                        />
                                    }
                                />
                                {/* TODO: show the actual Rewards Collected value https://github.com/MystenLabs/sui/issues/3605 */}
                                <CardItem
                                    title="EARNED"
                                    value={
                                        <CoinBalance
                                            balance={BigInt(0)}
                                            type={GAS_TYPE_ARG}
                                            mode="neutral"
                                            diffSymbol
                                            className="!text-gray-60"
                                            title="This value currently is not available"
                                        />
                                    }
                                />
                            </div>
                        </Card>

                        <div className="grid grid-cols-2 gap-2.5 mt-4">
                            {hasDelegations ? (
                                <>
                                    {pendingDelegations.map(
                                        (
                                            { name, staked, validatorAddress },
                                            index
                                        ) => (
                                            <DelegationCard
                                                key={index}
                                                name={name}
                                                staked={staked}
                                                state={DelegationState.WARM_UP}
                                                address={validatorAddress}
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
                                        {' '}
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
