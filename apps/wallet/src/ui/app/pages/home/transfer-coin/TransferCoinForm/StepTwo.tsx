// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';
import cl from 'classnames';
import { Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useMemo } from 'react';

import { parseAmount } from './utils';
import { Content, Menu } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import AddressInput from '_components/address-input';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useCoinDecimals, useFormatCoin } from '_hooks';
import { GAS_SYMBOL, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '../';

import st from './TransferCoinForm.module.scss';

export type TransferCoinFormProps = {
    submitError: string | null;
    coinSymbol: string;
    coinType: string;
    gasBudgetEstimation: number | null;
    gasCostEstimation: number | null;
    gasEstimationLoading: boolean;
    onClearSubmitError: () => void;
};

function StepTwo({
    submitError,
    coinSymbol,
    coinType,
    gasBudgetEstimation,
    gasCostEstimation,
    gasEstimationLoading,
    onClearSubmitError,
}: TransferCoinFormProps) {
    const {
        isSubmitting,
        isValid,
        isValidating,
        values: { amount, to },
    } = useFormikContext<FormValues>();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [amount, to]);
    const [decimals] = useCoinDecimals(coinType);
    const amountWithoutDecimals = useMemo(
        () => parseAmount(amount, decimals),
        [amount, decimals]
    );
    const totalSuiAmount = new BigNumber(gasCostEstimation || 0)
        .plus(GAS_SYMBOL === coinSymbol ? amountWithoutDecimals.toString() : 0)
        .toString();
    const submitBtnDisabled =
        !isValid ||
        to === '' ||
        isSubmitting ||
        isValidating ||
        !gasBudgetEstimation;
    const [formattedBalance] = useFormatCoin(amountWithoutDecimals, coinType);
    const [formattedTotalSui] = useFormatCoin(totalSuiAmount, GAS_TYPE_ARG);
    const [formattedGas] = useFormatCoin(gasCostEstimation, GAS_TYPE_ARG);

    return (
        <Form className={st.container} autoComplete="off" noValidate={true}>
            <Content>
                <div className={st.labelDirection}>
                    Enter or search the address of the recepient below to start
                    sending coins.
                </div>
                <div className={cl(st.group, st.address)}>
                    <Field
                        component={AddressInput}
                        name="to"
                        className={st.input}
                    />
                </div>

                {submitError ? (
                    <div className="mt-[10px]">
                        <Alert>{submitError}</Alert>
                    </div>
                ) : null}

                <div className={st.responseCard}>
                    <div className={st.amount}>
                        {formattedBalance} <span>{coinSymbol}</span>
                    </div>

                    <div className={st.details}>
                        {[
                            ['Estimated Gas Fee', formattedGas, GAS_SYMBOL],
                            ['Total Amount', formattedTotalSui, GAS_SYMBOL],
                        ].map(([label, frmt, symbol]) => (
                            <div className={st.txFees} key={label}>
                                <div className={st.txInfoLabel}>{label}</div>
                                <div className={st.walletInfoValue}>
                                    {gasEstimationLoading &&
                                    !(
                                        gasBudgetEstimation || gasCostEstimation
                                    ) ? (
                                        <LoadingIndicator />
                                    ) : frmt ? (
                                        `${frmt} ${symbol}`
                                    ) : (
                                        '-'
                                    )}
                                </div>
                            </div>
                        ))}
                    </div>
                </div>
            </Content>
            <Menu stuckClass={st.shadow}>
                <div className={cl(st.group, st.cta)}>
                    <Button
                        type="submit"
                        disabled={submitBtnDisabled}
                        mode="primary"
                        className={st.btn}
                    >
                        {isSubmitting ||
                        (gasEstimationLoading && !gasBudgetEstimation) ? (
                            <LoadingIndicator />
                        ) : (
                            'Send Coins Now'
                        )}
                        <Icon
                            icon={SuiIcons.ArrowLeft}
                            className={cl(st.arrowLeft)}
                        />
                    </Button>
                </div>
            </Menu>
        </Form>
    );
}

export default memo(StepTwo);
