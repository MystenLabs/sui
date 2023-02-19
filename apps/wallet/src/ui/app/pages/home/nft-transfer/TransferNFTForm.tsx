// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import cl from 'classnames';
import { Form, Field, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useMemo } from 'react';

import { Content } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import AddressInput from '_components/address-input';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useGetCoinBalance, useAppSelector } from '_hooks';
import { useGasBudgetInMist } from '_src/ui/app/hooks/useGasBudgetInMist';

import type { FormValues } from '.';

import st from './TransferNFTForm.module.scss';

export type TransferNFTFormProps = {
    submitError: string | null;
    gasBudget: number;
    onClearSubmitError: () => void;
};

function TransferNFTForm({
    submitError,
    gasBudget,
    onClearSubmitError,
}: TransferNFTFormProps) {
    const {
        isSubmitting,
        isValid,
        values: { to },
    } = useFormikContext<FormValues>();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [to]);

    const accountAddress = useAppSelector(({ account }) => account.address);
    const { data: coinBalance, isLoading: loadingBalances } = useGetCoinBalance(
        { address: accountAddress, coinType: SUI_TYPE_ARG }
    );

    const maxGasCoinBalance = useMemo(
        () => coinBalance?.totalBalance || BigInt(0),
        [coinBalance]
    );

    const { gasBudget: gasBudgetInMist } = useGasBudgetInMist(gasBudget);
    const isInsufficientGas = maxGasCoinBalance < BigInt(gasBudgetInMist || 0);
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
                        <div className="mt-2.5">
                            <Alert>
                                Insufficient balance, no individual coin found
                                with enough balance to cover for the transfer
                                cost
                            </Alert>
                        </div>
                    ) : null}
                    {submitError ? (
                        <div className="mt-2.5">
                            <Alert>{submitError}</Alert>
                        </div>
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
                                !gasBudgetInMist ||
                                loadingBalances
                            }
                            className={cl(st.action, 'btn', st.sendNftBtn)}
                        >
                            {isSubmitting ? (
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
