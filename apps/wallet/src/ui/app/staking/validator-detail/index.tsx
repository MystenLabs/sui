// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';
import { useSearchParams, useNavigate, Navigate } from 'react-router-dom';

import { StakingReward } from './StakingRewards';
import BottomMenuLayout, { Content } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import Card, { CardContent, CardItem, CardHeader } from '_app/shared/card';
import CoinBalance from '_app/shared/coin-balance';
import { ImageIcon } from '_app/shared/image-icon';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import { totalActiveStakedSelector } from '_app/staking/selectors';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useAppSelector, useObjectsState, useGetValidators } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

const APY_TOOLTIP = 'Annual Percentage Yield';
const COMMISSION_TOOLTIP = 'Validator commission';

export function ValidatorDetail() {
    const { loading, error, showError } = useObjectsState();
    const [searchParams] = useSearchParams();
    const validatorAddress = searchParams.get('address');
    const [showModal, setShowModal] = useState(true);

    const accountAddress = useAppSelector(({ account }) => account.address);
    const { validators, isLoading } = useGetValidators(accountAddress);

    const validatorDataByAddress = validators.find(
        ({ address }) => address === validatorAddress
    );

    const navigate = useNavigate();
    const close = useCallback(() => {
        navigate('/');
    }, [navigate]);

    if (!validatorAddress || (!validatorDataByAddress && !isLoading)) {
        return <Navigate to={'/stake'} replace={true} />;
    }

    const validatorName = validatorDataByAddress?.name || 'Loading...';

    const pageTitle = (
        <div className="flex gap-2 items-center capitalize">
            {validatorDataByAddress?.logo && (
                <ImageIcon
                    src={validatorDataByAddress.logo}
                    alt={validatorName}
                    variant="rounded"
                />
            )}
            {validatorName}
        </div>
    );

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={pageTitle}
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            <Loading
                className="w-full flex justify-center items-center"
                loading={loading || isLoading}
            >
                {showError && error && (
                    <Alert className="mb-2">
                        <strong>Sync error (data might be outdated).</strong>
                        <small>{error.message}</small>
                    </Alert>
                )}

                {validatorDataByAddress && (
                    <ValidatorDetailCard
                        validatorAddress={validatorAddress}
                        pendingDelegationAmount={
                            validatorDataByAddress.pendingDelegationAmount
                        }
                        suiEarned={validatorDataByAddress.suiEarned}
                        apy={validatorDataByAddress.apy}
                        commissionRate={validatorDataByAddress.commissionRate}
                    />
                )}
            </Loading>
        </Overlay>
    );
}

type ValidatorDetailCardProps = {
    validatorAddress: string;
    pendingDelegationAmount: bigint;
    suiEarned: bigint;
    apy: number | string;
    commissionRate: number;
};

function ValidatorDetailCard({
    validatorAddress,
    pendingDelegationAmount,
    suiEarned,
    apy,
    commissionRate,
}: ValidatorDetailCardProps) {
    const totalStaked = useAppSelector(totalActiveStakedSelector);
    const pendingStake = pendingDelegationAmount || 0n;
    const totalStakedIncludingPending = totalStaked + pendingStake;

    const stakeByValidatorAddress = `/stake/new?address=${encodeURIComponent(
        validatorAddress
    )}`;

    const apyTitle = (
        <div className="flex text-steel-darker gap-1 items-start">
            APY
            <div className="text-steel">
                <IconTooltip tip={APY_TOOLTIP} placement="top" />
            </div>
        </div>
    );

    const commissionTitle = (
        <div className="flex text-steel-darker gap-1">
            Commission
            <div className="text-steel">
                <IconTooltip tip={COMMISSION_TOOLTIP} placement="top" />
            </div>
        </div>
    );
    return (
        <div className="flex flex-col flex-nowrap flex-grow h-full">
            <BottomMenuLayout>
                <Content>
                    <div className="justify-center w-full flex flex-col items-center">
                        <Card className="mb-4 w-full">
                            <CardHeader>
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

                                <CardItem
                                    title="EARNED"
                                    value={
                                        <CoinBalance
                                            balance={suiEarned}
                                            type={GAS_TYPE_ARG}
                                            mode="positive"
                                            diffSymbol={true}
                                            title="This value currently is not available"
                                        />
                                    }
                                />
                            </CardHeader>
                            <CardContent>
                                <CardItem
                                    title={apyTitle}
                                    value={
                                        <div className="flex gap-0.5 items-baseline ">
                                            <Text
                                                variant="heading4"
                                                weight="semibold"
                                                color="gray-90"
                                            >
                                                {apy}
                                            </Text>

                                            <Text
                                                variant="subtitleSmall"
                                                weight="medium"
                                                color="steel-dark"
                                            >
                                                %
                                            </Text>
                                        </div>
                                    }
                                />

                                <CardItem
                                    title={commissionTitle}
                                    value={
                                        <div className="flex gap-0.5 items-baseline ">
                                            <Text
                                                variant="heading4"
                                                weight="semibold"
                                                color="gray-90"
                                            >
                                                {commissionRate}
                                            </Text>

                                            <Text
                                                variant="subtitleSmall"
                                                weight="medium"
                                                color="steel-dark"
                                            >
                                                %
                                            </Text>
                                        </div>
                                    }
                                />
                            </CardContent>
                        </Card>
                        <div className="flex gap-2.5 mb-8 w-full">
                            <Button
                                size="large"
                                mode="outline"
                                href={stakeByValidatorAddress}
                                className="bg-gray-50 w-full"
                            >
                                <Icon icon={SuiIcons.Add} />
                                Stake SUI
                            </Button>
                            {totalStakedIncludingPending > 0 && (
                                <Button
                                    size="large"
                                    mode="outline"
                                    disabled={true}
                                    href={
                                        stakeByValidatorAddress + '&unstake=1'
                                    }
                                    className="w-full"
                                >
                                    <Icon
                                        icon={SuiIcons.Remove}
                                        className="text-heading6"
                                    />
                                    Unstake SUI
                                </Button>
                            )}
                        </div>
                    </div>
                    <StakingReward />
                </Content>
                <Button
                    size="large"
                    mode="neutral"
                    href="/stake"
                    className="!text-steel-darker"
                >
                    <Icon
                        icon={SuiIcons.ArrowLeft}
                        className="text-body text-gray-60 font-normal"
                    />
                    Back
                </Button>
            </BottomMenuLayout>
        </div>
    );
}
