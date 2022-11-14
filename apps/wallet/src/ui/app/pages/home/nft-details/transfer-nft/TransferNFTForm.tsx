// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import cl from 'classnames';
import { Form, Field, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useMemo } from 'react';

import { Content } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import AddressInput from '_components/address-input';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useAppSelector, useSigner } from '_hooks';
import { accountAggregateBalancesSelector } from '_redux/slices/account';
import {
    DEFAULT_NFT_TRANSFER_GAS_FEE,
    GAS_TYPE_ARG,
} from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '.';
import type { ObjectId } from '@mysten/sui.js';

import st from './TransferNFTForm.module.scss';

export type TransferNFTFormProps = {
    nftID: ObjectId;
    submitError: string | null;
    onClearSubmitError: () => void;
};

function TransferNFTForm({
    nftID,
    submitError,
    onClearSubmitError,
}: TransferNFTFormProps) {
    const {
        isSubmitting,
        isValid,
        isValidating,
        values: { to },
    } = useFormikContext<FormValues>();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [to]);
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const gasAggregateBalance = useMemo(
        () => aggregateBalances[GAS_TYPE_ARG] || BigInt(0),
        [aggregateBalances]
    );
    const signer = useSigner();
    const gasEstimationEnabled = !!(isValid && !isValidating && nftID && to);
    const gasEstimationResult = useQuery({
        queryKey: ['nft-transfer', nftID, 'gas-estimation', to],
        queryFn: async () => {
            const tx = await signer.serializer.newTransferObject(
                await signer.getAddress(),
                {
                    objectId: nftID,
                    recipient: to,
                    gasBudget: DEFAULT_NFT_TRANSFER_GAS_FEE,
                }
            );
            return signer.getGasCostEstimation(tx);
        },
        enabled: gasEstimationEnabled,
    });
    const gasEstimation = gasEstimationResult.isError
        ? DEFAULT_NFT_TRANSFER_GAS_FEE
        : gasEstimationResult.data ?? null; // make undefined null
    const isInsufficientGas =
        gasEstimation !== null ? gasAggregateBalance < gasEstimation : null;
    return (
        <div className={st.sendNft}>
            <Content>
                <Form
                    className={st.container}
                    autoComplete="off"
                    noValidate={true}
                >
                    <label className={st.labelInfo}>
                        Enter the address of the recipient to start sending the
                        NFT
                    </label>
                    <div className={st.group}>
                        <Field
                            component={AddressInput}
                            name="to"
                            as="div"
                            id="to"
                            placeholder="Enter Address"
                            className={st.input}
                        />
                    </div>
                    {isInsufficientGas ? (
                        <div className={st.error}>
                            * Insufficient balance to cover transfer cost
                        </div>
                    ) : null}
                    {submitError ? (
                        <div className={st.error}>{submitError}</div>
                    ) : null}
                    <div className={st.formcta}>
                        <Button
                            size="large"
                            mode="primary"
                            type="submit"
                            disabled={
                                !isValid ||
                                isSubmitting ||
                                isInsufficientGas ||
                                gasEstimationResult.isLoading
                            }
                            className={cl(st.action, 'btn', st.sendNftBtn)}
                        >
                            {isSubmitting ||
                            (gasEstimationEnabled &&
                                gasEstimationResult.isLoading) ? (
                                <LoadingIndicator />
                            ) : (
                                <>
                                    Send NFT Now
                                    <Icon
                                        icon={SuiIcons.ArrowRight}
                                        className={st.arrowActionIcon}
                                    />
                                </>
                            )}
                        </Button>
                    </div>
                </Form>
            </Content>
        </div>
    );
}

export default memo(TransferNFTForm);
