// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import cl from 'classnames';
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

import st from './StakeHome.module.scss';

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
            <div className={cl(st.container, 'w-full')}>
                {showError && error ? (
                    <Alert className="mb-2">
                        <strong>Sync error (data might be outdated).</strong>{' '}
                        <small>{error.message}</small>
                    </Alert>
                ) : null}
                <Loading
                    loading={loading || pendingDelegationsLoading}
                    className={st.stakedInfoContainer}
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

    const numOfValidators = [
        new Set([...activeDelegationIDs, ...pendingDelegations]),
    ];

    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

    return (
        <div className={st.container}>
            <BottomMenuLayout>
                <Content>
                    <div className="mb-4">
                        <Card
                            header={
                                <Text
                                    variant="captionSmall"
                                    weight="medium"
                                    color="steel-darker"
                                >
                                    STAKING ON {numOfValidators.length}{' '}
                                    VALIDATORS
                                </Text>
                            }
                        >
                            <div>
                                <CardItem
                                    title="Your Stake"
                                    value={
                                        <CoinBalance
                                            balance={
                                                totalStakedIncludingPending
                                            }
                                            type={GAS_TYPE_ARG}
                                            diffSymbol={true}
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
                                            mode="positive"
                                            diffSymbol={true}
                                            title="This value currently is not available"
                                        />
                                    }
                                />
                            </div>
                        </Card>

                        <div className={st.stakedContainer}>
                            {hasDelegations ? (
                                <>
                                    {pendingDelegations.map(
                                        ({ name, staked }, index) => (
                                            <DelegationCard
                                                key={index}
                                                name={name}
                                                staked={staked}
                                                state={DelegationState.WARM_UP}
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
                                <div
                                    className={cl(
                                        st.stakedInfoContainer,
                                        st.empty
                                    )}
                                >
                                    No active stakes found
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
