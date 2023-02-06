// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getTransactionDigest,
    SUI_TYPE_ARG,
    normalizeSuiAddress,
    type SuiAddress,
} from '@mysten/sui.js';
import { useQueryClient, useMutation } from '@tanstack/react-query';
import { Formik } from 'formik';
import { useCallback, useMemo } from 'react';
import { toast } from 'react-hot-toast';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import Alert from '../../components/alert';
import { useReferenceGasPrice } from '../../hooks/useReferenceGasPrice';
import { getStakingRewards } from '../getStakingRewards';
import { useGetDelegatedStake } from '../useGetDelegatedStake';
import { STATE_OBJECT } from '../usePendingDelegation';
import { validatorsFields } from '../validatorsFields';
import { DelegationState, STATE_TO_COPY } from './../home/DelegationCard';
import StakeForm from './StakeForm';
import { UnStakeForm } from './UnstakeForm';
import { ValidatorFormDetail } from './ValidatorFormDetail';
import { createValidationSchema } from './validation';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { Collapse } from '_app/shared/collapse';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { parseAmount } from '_helpers';
import {
    useSigner,
    useAppSelector,
    useCoinDecimals,
    useGetObject,
} from '_hooks';
import {
    accountAggregateBalancesSelector,
    createCoinsForTypeSelector,
} from '_redux/slices/account';
import { Coin, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { trackEvent } from '_src/shared/plausible';
import { Text } from '_src/ui/app/shared/text';

import type { FormikHelpers } from 'formik';

const initialValues = {
    amount: '',
    gasBudget: '',
};

export type FormValues = typeof initialValues;

function StakingCard() {
    const coinType = GAS_TYPE_ARG;
    const accountAddress = useAppSelector(({ account }) => account.address);
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const [searchParams] = useSearchParams();
    const validatorAddress = searchParams.get('address');
    const stakeIdParams = searchParams.get('staked');
    const unstake = searchParams.get('unstake') === 'true';
    const { data: allDelegation, isLoading } = useGetDelegatedStake(
        accountAddress || ''
    );

    //TODO: since we only require epoch number, probably we can use a different query
    const { data: validators, isLoading: validatorsIsloading } =
        useGetObject(STATE_OBJECT);

    const validatorsData = validators && validatorsFields(validators);

    const totalTokenBalance = useMemo(() => {
        if (!allDelegation) return 0n;
        // return only the total amount of tokens staked for a specific stakeId
        if (stakeIdParams) {
            const balance =
                allDelegation.find(
                    ({ staked_sui }) => staked_sui.id.id === stakeIdParams
                )?.staked_sui.principal.value || 0;
            return BigInt(balance);
        }
        // return aggregate delegation
        return allDelegation.reduce(
            (acc, { staked_sui }) => acc + BigInt(staked_sui.principal.value),
            0n
        );
    }, [allDelegation, stakeIdParams]);
    const coinBalance = (coinType && aggregateBalances[coinType]) || 0n;
    const delegationData = useMemo(() => {
        if (!allDelegation) return null;

        return allDelegation.find(
            ({ staked_sui }) => staked_sui.id.id === stakeIdParams
        );
    }, [allDelegation, stakeIdParams]);

    const coinSymbol = useMemo(
        () => (coinType && Coin.getCoinSymbol(coinType)) || '',
        [coinType]
    );

    const suiEarned = useMemo(() => {
        if (!validatorsData || !delegationData) return 0;
        return getStakingRewards(
            validatorsData.validators.fields.active_validators,
            delegationData
        );
    }, [delegationData, validatorsData]);

    const [coinDecimals] = useCoinDecimals(coinType);

    const validationSchema = useMemo(
        () =>
            createValidationSchema(
                coinBalance,
                coinSymbol,
                coinDecimals,
                unstake
            ),
        [coinBalance, coinSymbol, coinDecimals, unstake]
    );

    const queryClient = useQueryClient();
    const delegationId = useMemo(() => {
        if (!delegationData || delegationData.delegation_status === 'Pending')
            return null;
        return delegationData.delegation_status.Active.id.id;
    }, [delegationData]);

    const gasPrice = useReferenceGasPrice();
    const navigate = useNavigate();
    const signer = useSigner();
    const allSuiCoinsSelector = useMemo(
        () => createCoinsForTypeSelector(SUI_TYPE_ARG),
        []
    );
    const allSuiCoins = useAppSelector(allSuiCoinsSelector);
    const stakeToken = useMutation({
        mutationFn: async ({
            tokenTypeArg,
            amount,
            validatorAddress,
        }: {
            tokenTypeArg: string;
            amount: bigint;
            validatorAddress: SuiAddress;
        }) => {
            if (!validatorAddress || !amount || !tokenTypeArg || !signer) {
                throw new Error('Failed, missing required field');
            }
            trackEvent('Stake', {
                props: { validator: validatorAddress },
            });
            const response = await Coin.stakeCoin(
                signer,
                allSuiCoins,
                amount,
                validatorAddress,
                gasPrice.data!
            );
            return response;
        },
    });
    const unStakeToken = useMutation({
        mutationFn: async ({
            delegationId,
            stakeSuId,
        }: {
            delegationId: string;
            stakeSuId: string;
        }) => {
            if (!delegationId || !stakeSuId || !signer) {
                throw new Error(
                    'Failed, missing required field (!principalWithdrawAmount | delegationId | stakeSuId).'
                );
            }

            trackEvent('Unstake');

            const response = await Coin.unStakeCoin(
                signer,
                delegationId,
                stakeSuId
            );
            return response;
        },
    });

    const onHandleSubmit = useCallback(
        async (
            { amount }: FormValues,
            { resetForm }: FormikHelpers<FormValues>
        ) => {
            if (coinType === null || validatorAddress === null) {
                return;
            }
            try {
                const bigIntAmount = parseAmount(amount, coinDecimals);
                let response;
                let txDigest;
                if (unstake) {
                    // check for delegation data
                    if (
                        !delegationData ||
                        !stakeIdParams ||
                        delegationData.delegation_status === 'Pending'
                    ) {
                        return;
                    }
                    response = await unStakeToken.mutateAsync({
                        delegationId:
                            delegationData.delegation_status.Active.id.id,
                        stakeSuId: stakeIdParams,
                    });

                    txDigest = getTransactionDigest(response);
                } else {
                    response = await stakeToken.mutateAsync({
                        amount: bigIntAmount,
                        tokenTypeArg: coinType,
                        validatorAddress: validatorAddress,
                    });
                    txDigest = getTransactionDigest(response);
                }

                // Invalidate the react query for 0x5 and validator
                Promise.all([
                    queryClient.invalidateQueries({
                        queryKey: ['object', normalizeSuiAddress(STATE_OBJECT)],
                    }),
                    queryClient.invalidateQueries({
                        queryKey: ['validator'],
                    }),
                ]);
                resetForm();

                navigate(
                    `/receipt?${new URLSearchParams({
                        txdigest: txDigest,
                    }).toString()}`
                );
            } catch (e) {
                const msg = (e as Error)?.message;
                toast.error(
                    <div className="max-w-xs overflow-hidden flex flex-col">
                        <strong>{unstake ? 'Unstake' : 'Stake'} failed</strong>
                        {msg ? (
                            <small className="text-ellipsis overflow-hidden">
                                {msg}
                            </small>
                        ) : null}
                    </div>
                );
            }
        },
        [
            coinType,
            validatorAddress,
            coinDecimals,
            unstake,
            queryClient,
            navigate,
            delegationData,
            stakeIdParams,
            unStakeToken,
            stakeToken,
        ]
    );
    const loadingBalance = useAppSelector(
        ({ suiObjects }) => suiObjects.loading && !suiObjects.lastSync
    );
    if (
        !coinType ||
        !validatorAddress ||
        (!validatorsIsloading && !validatorsData)
    ) {
        return <Navigate to="/" replace={true} />;
    }
    return (
        <div className="flex flex-col flex-nowrap flex-grow w-full">
            <Loading
                loading={loadingBalance || isLoading || validatorsIsloading}
                className="flex justify-center w-full h-full items-center"
            >
                <Formik
                    initialValues={initialValues}
                    validationSchema={validationSchema}
                    onSubmit={onHandleSubmit}
                >
                    {({
                        isSubmitting,
                        isValid,
                        submitForm,
                        errors,
                        touched,
                    }) => (
                        <BottomMenuLayout>
                            <Content>
                                <div className="mb-4">
                                    <ValidatorFormDetail
                                        validatorAddress={validatorAddress}
                                        unstake={unstake}
                                        stakedId={stakeIdParams}
                                    />
                                </div>

                                {unstake ? (
                                    <UnStakeForm
                                        coinBalance={totalTokenBalance}
                                        coinType={coinType}
                                        stakingReward={suiEarned}
                                    />
                                ) : (
                                    <StakeForm
                                        coinBalance={coinBalance}
                                        coinType={coinType}
                                        epoch={validatorsData?.epoch}
                                    />
                                )}

                                {(unstake || touched.amount) &&
                                (errors.amount || errors.gasBudget) ? (
                                    <div className="mt-2 flex flex-col flex-nowrap">
                                        <Alert
                                            mode="warning"
                                            className="text-body"
                                        >
                                            {errors.amount || errors.gasBudget}
                                        </Alert>
                                    </div>
                                ) : null}

                                {!unstake && (
                                    <div className="flex-1 mt-7.5">
                                        <Collapse
                                            title={
                                                STATE_TO_COPY[
                                                    delegationData?.delegation_status ===
                                                    'Pending'
                                                        ? DelegationState.WARM_UP
                                                        : DelegationState.EARNING
                                                ]
                                            }
                                            initialIsOpen
                                        >
                                            <Text
                                                variant="p3"
                                                color="steel-dark"
                                                weight="normal"
                                            >
                                                The staked SUI starts earning
                                                reward at the end of the Epoch
                                                in which it was staked. The
                                                rewards will become available at
                                                the end of one full Epoch of
                                                staking.
                                            </Text>
                                        </Collapse>
                                    </div>
                                )}
                            </Content>

                            <Menu
                                stuckClass="staked-cta"
                                className="w-full px-0 pb-0 mx-0"
                            >
                                <Button
                                    size="large"
                                    mode="neutral"
                                    href="/stake"
                                    disabled={isSubmitting}
                                    className="!text-steel-darker w-1/2"
                                >
                                    <Icon
                                        icon={SuiIcons.ArrowLeft}
                                        className="text-body text-gray-65 font-normal"
                                    />
                                    Back
                                </Button>
                                <Button
                                    size="large"
                                    mode="primary"
                                    onClick={submitForm}
                                    className="w-1/2"
                                    disabled={
                                        !isValid ||
                                        isSubmitting ||
                                        (unstake && !delegationId) ||
                                        gasPrice.isLoading
                                    }
                                >
                                    {isSubmitting ? (
                                        <LoadingIndicator className="border-white" />
                                    ) : unstake ? (
                                        'Unstake Now'
                                    ) : (
                                        'Stake Now'
                                    )}
                                </Button>
                            </Menu>
                        </BottomMenuLayout>
                    )}
                </Formik>
            </Loading>
        </div>
    );
}

export default StakingCard;
