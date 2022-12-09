// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import cl from 'classnames';
import { useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

import { FEATURES } from '../../experimentation/features';
import { Text } from '../../shared/Text';
import { usePendingDelegation } from '../usePendingDelegation';
import { ActiveDelegation } from './ActiveDelegation';
import { ActiveValidatorsCard } from './ActiveValidatorsCard';
import { DelegationCard, DelegationState } from './DelegationCard';
import BottomMenuLayout, { Content } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import Card, { CardRow, CardItem, CardHeader } from '_app/shared/card';
import CoinBalance from '_app/shared/coin-balance';
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

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title="Select a Validator"
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            <div className={cl(st.container, 'w-full')}>
                {showError && error ? (
                    <Alert className={st.alert}>
                        <strong>Sync error (data might be outdated).</strong>{' '}
                        <small>{error.message}</small>
                    </Alert>
                ) : null}
                <Loading
                    loading={loading || pendingDelegationsLoading}
                    className={st.stakedInfoContainer}
                >
                    {hasDelegations ? (
                        <StakedCard
                            activeDelegationIDs={activeDelegationIDs}
                            pendingDelegations={pendingDelegations}
                        />
                    ) : (
                        <ActiveValidatorsCard />
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

function StakedCard({
    activeDelegationIDs,
    pendingDelegations,
}: StakedCardProps) {
    const totalStaked = useAppSelector(totalActiveStakedSelector);
    const totalStakedIncludingPending =
        totalStaked +
        pendingDelegations.reduce((acc, { staked }) => acc + staked, 0n);

    const hasDelegations =
        activeDelegationIDs.length > 0 || pendingDelegations.length > 0;

    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

    return (
        <div className={st.container}>
            <BottomMenuLayout>
                <Content>
                    <Card className="mb-4">
                        <CardHeader>
                            <Text
                                variant="captionSmall"
                                weight="medium"
                                color="steel-darker"
                            >
                                STAKING ON 4 VALIDATORS
                            </Text>
                        </CardHeader>

                        <CardRow>
                            <CardItem
                                title="Your Stake"
                                value={
                                    <CoinBalance
                                        balance={totalStakedIncludingPending}
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
                        </CardRow>
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
                                            address=""
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
                                className={cl(st.stakedInfoContainer, st.empty)}
                            >
                                No active stakes found
                            </div>
                        )}
                    </div>
                </Content>
                <Button
                    size="large"
                    mode="neutral"
                    href="new"
                    title="Currently disabled"
                    disabled={!stakingEnabled}
                >
                    Stake Coins
                    <Icon
                        icon={SuiIcons.ArrowRight}
                        className={st.arrowActionIcon}
                    />
                </Button>
            </BottomMenuLayout>
        </div>
    );
}

export default StakeHome;
