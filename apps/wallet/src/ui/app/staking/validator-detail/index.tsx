// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';
import { useSearchParams, useNavigate, Navigate } from 'react-router-dom';

import { usePendingDelegation } from '../usePendingDelegation';
import BottomMenuLayout, { Content } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Card } from '_app/shared/card';
import { CardItem } from '_app/shared/card/CardItem';
import CoinBalance from '_app/shared/coin-balance';
import { ImageIcon } from '_app/shared/image-icon';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import { totalActiveStakedSelector } from '_app/staking/selectors';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useAppSelector, useObjectsState } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

const APY_TOOLTIP = 'Annual Percentage Yield';
const COMMISSION_TOOLTIP = 'Validator commission';

export function ValidatorDetail() {
    const { loading, error, showError } = useObjectsState();
    const [searchParams] = useSearchParams();
    const validatorAddressParams = searchParams.get('address');
    const [showModal, setShowModal] = useState(true);
    const [pendingDelegations, { isLoading: pendingDelegationsLoading }] =
        usePendingDelegation();

    const navigate = useNavigate();
    const close = useCallback(() => {
        navigate('/');
    }, [navigate]);

    if (!validatorAddressParams) {
        return <Navigate to={'/stake'} replace={true} />;
    }
    const validatorData = pendingDelegations.find(
        ({ validatorAddress }) => validatorAddress === validatorAddressParams
    );

    const validatorName = validatorData?.name || 'Loading...';

    // TODO: get logo from validator data
    const logo = null;

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={
                <div className="flex gap-2 items-center capitalize">
                    <ImageIcon src={logo} alt={validatorName} size="small" />

                    {validatorName}
                </div>
            }
            closeIcon={SuiIcons.Close}
            closeOverlay={close}
        >
            <Loading
                className="w-full flex justify-center items-center"
                loading={loading || pendingDelegationsLoading}
            >
                {showError && error && (
                    <Alert className="mb-2">
                        <strong>Sync error (data might be outdated).</strong>
                        <small>{error.message}</small>
                    </Alert>
                )}

                {validatorData && (
                    <ValidatorDetailCard
                        validatorAddress={validatorData.validatorAddress}
                        pendingDelegationAmount={validatorData.staked || 0n}
                        suiEarned={0n}
                        apy={'0.00%'}
                        commissionRate={0}
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

    const cardHeader = (
        <div className="grid grid-cols-2 divide-x divide-solid divide-gray-45 divide-y-0 w-full">
            <CardItem
                title="Your Stake"
                value={
                    <CoinBalance
                        balance={totalStakedIncludingPending}
                        type={GAS_TYPE_ARG}
                        diffSymbol
                    />
                }
            />

            <CardItem
                title="EARNED"
                value={
                    <CoinBalance
                        balance={suiEarned}
                        type={GAS_TYPE_ARG}
                        mode="neutral"
                        className="!text-gray-60"
                        diffSymbol
                        title="This value currently is not available"
                    />
                }
            />
        </div>
    );

    return (
        <div className="flex flex-col flex-nowrap flex-grow h-full">
            <BottomMenuLayout>
                <Content>
                    <div className="justify-center w-full flex flex-col items-center">
                        <div className="mb-4 w-full flex">
                            <Card header={cardHeader} padding="none">
                                <div className="divide-x flex divide-solid divide-gray-45 divide-y-0">
                                    <CardItem
                                        title={
                                            <div className="flex text-steel-darker gap-1 items-start">
                                                APY
                                                <div className="text-steel">
                                                    <IconTooltip
                                                        tip={APY_TOOLTIP}
                                                        placement="top"
                                                    />
                                                </div>
                                            </div>
                                        }
                                        value={
                                            <div className="flex gap-0.5 items-baseline">
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
                                        title={
                                            <div className="flex text-steel-darker gap-1">
                                                Commission
                                                <div className="text-steel">
                                                    <IconTooltip
                                                        tip={COMMISSION_TOOLTIP}
                                                        placement="top"
                                                    />
                                                </div>
                                            </div>
                                        }
                                        value={
                                            <div className="flex gap-0.5 items-baseline">
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
                                </div>
                            </Card>
                        </div>
                        <div className="flex gap-2.5 mb-8 w-full mt-4">
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
