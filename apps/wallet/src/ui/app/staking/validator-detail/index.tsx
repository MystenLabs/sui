// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '@mysten/sui.js';
import { useCallback, useState } from 'react';
import { useSearchParams, useNavigate, Navigate } from 'react-router-dom';

import { Text } from '../../shared/Text';
// import { IconTooltip } from '../../shared/Tooltip';
import { ImageIcon } from '../../shared/image-icon';
import { usePendingDelegation } from '../usePendingDelegation';
import BottomMenuLayout, { Content } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import Card, { CardRow, CardItem, CardHeader } from '_app/shared/card';
import CoinBalance from '_app/shared/coin-balance';
import {
    //  activeDelegationIDsSelector,
    getValidatorSelector,
    totalActiveStakedSelector,
} from '_app/staking/selectors';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { useAppSelector, useObjectsState } from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

const textDecoder = new TextDecoder();

export function ValidatorDetail() {
    const { loading, error, showError } = useObjectsState();
    const [searchParams] = useSearchParams();
    const validatorAddress = searchParams.get('address');
    const [showModal, setShowModal] = useState(true);

    const navigate = useNavigate();
    const close = useCallback(() => {
        navigate('/stake');
    }, [navigate]);

    const validatorDataByAddress = useAppSelector(
        getValidatorSelector(validatorAddress || '')
    );

    const validatorName = textDecoder.decode(
        new Base64DataBuffer(validatorDataByAddress?.fields.name).getData()
    );

    if (!validatorDataByAddress && !loading) {
        return <Navigate to={'/stake'} replace={true} />;
    }

    const logo = null;
    const pageTitle = (
        <div className="flex gap-2 items-center capitalize">
            <ImageIcon
                src={logo}
                alt={validatorName}
                fillers={!!logo}
                variant="rounded"
            />
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
            <div className=" w-full ">
                {showError && error ? (
                    <Alert className="mb-2">
                        <strong>Sync error (data might be outdated).</strong>
                        <small>{error.message}</small>
                    </Alert>
                ) : null}
                <ValidatorDetailCard
                    validatorAddress={validatorAddress || ''}
                />
            </div>
        </Overlay>
    );
}

function ValidatorDetailCard({
    validatorAddress,
}: {
    validatorAddress: string;
}) {
    // const activeDelegationIDs = useAppSelector(activeDelegationIDsSelector);
    /*const validatorByAddress = useAppSelector(
        getValidatorSelector(validatorAddress)
    );*/

    const [pendingDelegations, { isLoading: pendingDelegationsLoading }] =
        usePendingDelegation();
    const totalStaked = useAppSelector(totalActiveStakedSelector);
    const totalStakedIncludingPending =
        totalStaked +
        pendingDelegations.reduce((acc, { staked }) => acc + staked, 0n);

    const apy = 7.5;
    const commission_rate = 0.42;

    return (
        <BottomMenuLayout>
            <Content>
                <Loading
                    loading={pendingDelegationsLoading}
                    className="justify-center w-full h-full flex items-center"
                >
                    <Card className="mb-4">
                        <CardHeader>
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
                        </CardHeader>
                        <CardRow>
                            <CardItem
                                title="APY"
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
                                title="Commission"
                                value={
                                    <div className="flex gap-0.5 items-baseline ">
                                        <Text
                                            variant="heading4"
                                            weight="semibold"
                                            color="gray-90"
                                        >
                                            {commission_rate}
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
                        </CardRow>
                    </Card>
                    <div className="flex gap-2.5 mb-4">
                        <Button
                            size="large"
                            mode="outline"
                            href="new"
                            className="bg-gray-50 w-1/2"
                        >
                            Stake SUI
                            <Icon icon={SuiIcons.ArrowRight} />
                        </Button>
                        <Button
                            size="large"
                            mode="outline"
                            href="new"
                            className="bg-gray-50 w-1/2 border border-steel-dark text-steel-dark"
                        >
                            Unstake SUI
                            <Icon
                                icon={SuiIcons.ArrowRight}
                                className="text-caption font-thin"
                            />
                        </Button>
                    </div>
                </Loading>
            </Content>

            <Button
                size="large"
                mode="neutral"
                href="new"
                title="Currently disabled"
            >
                Back
                <Icon icon={SuiIcons.ArrowRight} />
            </Button>
        </BottomMenuLayout>
    );
}
