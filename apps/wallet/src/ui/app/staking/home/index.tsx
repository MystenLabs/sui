// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import cl from 'classnames';

import { FEATURES } from '../../experimentation/features';
import DelegationCard from './delegation-card';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import CoinBalance from '_app/shared/coin-balance';
import PageTitle from '_app/shared/page-title';
import StatsCard, { StatsRow, StatsItem } from '_app/shared/stats-card';
import {
    activeDelegationIDsSelector,
    totalActiveStakedSelector,
} from '_app/staking/selectors';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { useAppSelector, useObjectsState } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import st from './StakeHome.module.scss';

function StakeHome() {
    const { loading, error, showError } = useObjectsState();
    const totalStaked = useAppSelector(totalActiveStakedSelector);
    const activeDelegationIDs = useAppSelector(activeDelegationIDsSelector);
    const stakingEnabled = useFeature(FEATURES.STAKING_ENABLED).on;

    return (
        <div className={st.container}>
            <PageTitle title="Stake & Earn" className={st.pageTitle} />
            {showError && error ? (
                <Alert className={st.alert}>
                    <strong>Sync error (data might be outdated).</strong>{' '}
                    <small>{error.message}</small>
                </Alert>
            ) : null}
            <BottomMenuLayout>
                <Content>
                    <div className={st.pageDescription}>
                        Staking SUI provides SUI holders with rewards in
                        addition to market price gains.
                    </div>
                    <StatsCard className={st.stats}>
                        <StatsRow>
                            <StatsItem
                                title="Total Staked"
                                value={
                                    <Loading loading={loading}>
                                        <CoinBalance
                                            balance={totalStaked}
                                            type={GAS_TYPE_ARG}
                                            diffSymbol={true}
                                        />
                                    </Loading>
                                }
                            />
                            {/* TODO: show the actual Rewards Collected value https://github.com/MystenLabs/sui/issues/3605 */}
                            <StatsItem
                                title="Rewards Collected"
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
                        </StatsRow>
                    </StatsCard>
                    <div className={st.titleSectionContainer}>
                        <span className={st.sectionTitle}>
                            Currently Staking
                        </span>
                        <Button
                            size="small"
                            mode="primary"
                            disabled={true}
                            title="Currently not available"
                        >
                            <span>Claim All Rewards</span>
                            <Icon
                                icon={SuiIcons.ArrowRight}
                                className={st.arrowIcon}
                            />
                        </Button>
                    </div>
                    <div className={st.stakedContainer}>
                        {
                            <Loading
                                loading={loading}
                                className={st.stakedInfoContainer}
                            >
                                {activeDelegationIDs.length ? (
                                    activeDelegationIDs.map((delegationID) => (
                                        <DelegationCard
                                            key={delegationID}
                                            id={delegationID}
                                        />
                                    ))
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
                            </Loading>
                        }
                    </div>
                </Content>
                <Menu stuckClass={st.shadow}>
                    <Button
                        size="large"
                        mode="neutral"
                        className={st.action}
                        href="/tokens"
                    >
                        <Icon
                            icon={SuiIcons.Close}
                            className={st.closeActionIcon}
                        />
                        Cancel
                    </Button>
                    <Button
                        size="large"
                        mode="primary"
                        className={st.action}
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
                </Menu>
            </BottomMenuLayout>
        </div>
    );
}

export default StakeHome;
