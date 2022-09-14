// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin, COIN_DENOMINATIONS } from '@mysten/sui.js';
import cl from 'classnames';
import { Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useMemo } from 'react';

import { Content, Menu } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import { useCoinFormat } from '_app/shared/coin-balance/coin-format';
import AddressInput from '_components/address-input';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import {
    DEFAULT_GAS_BUDGET_FOR_TRANSFER,
    GAS_TYPE_ARG,
} from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '_pages/home/transfer-coin';

import st from './TransferCoinForm.module.scss';

export type TransferCoinFormProps = {
    submitError: string | null;
    coinType: string;
    onClearSubmitError: () => void;
};

function StepTwo({
    submitError,
    coinType,
    onClearSubmitError,
}: TransferCoinFormProps) {
    const {
        isSubmitting,
        isValid,
        values: { amount, to },
    } = useFormikContext<FormValues>();
    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [amount, to]);
    // TODO: this should be provided from the input component
    const coinInputDenomination = useMemo(
        () =>
            coinType in COIN_DENOMINATIONS ? COIN_DENOMINATIONS[coinType] : 1,
        [coinType]
    );
    const amountValue = useMemo(
        () => Coin.fromInput(amount, coinInputDenomination),
        [amount, coinInputDenomination]
    );
    const amountFormatData = useCoinFormat(amountValue, coinType, 'accurate');
    const totalAmount = amountValue + BigInt(DEFAULT_GAS_BUDGET_FOR_TRANSFER);
    const totalAmountFormatted = useCoinFormat(
        totalAmount,
        coinType,
        'accurate'
    ).displayFull;
    const validAddressBtn = !isValid || to === '' || isSubmitting;
    const gasCostFormatted = useCoinFormat(
        BigInt(DEFAULT_GAS_BUDGET_FOR_TRANSFER),
        GAS_TYPE_ARG,
        'accurate'
    ).displayFull;

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
                    <div className={st.error}>{submitError}</div>
                ) : null}

                <div className={st.responseCard}>
                    <div className={st.amount}>
                        {amountFormatData.displayBalance}{' '}
                        <span>{amountFormatData.symbol}</span>
                    </div>
                    <div className={st.details}>
                        <div className={st.txFees}>
                            <div className={st.txInfoLabel}>Gas Fee</div>
                            <div className={st.walletInfoValue}>
                                {gasCostFormatted}
                            </div>
                        </div>
                        <div className={st.txFees}>
                            <div className={st.txInfoLabel}>Total Amount</div>
                            <div className={st.walletInfoValue}>
                                {totalAmountFormatted}
                            </div>
                        </div>
                    </div>
                </div>
            </Content>
            <Menu stuckClass={st.shadow}>
                <div className={cl(st.group, st.cta)}>
                    <Button
                        type="submit"
                        disabled={validAddressBtn}
                        mode="primary"
                        className={st.btn}
                    >
                        {isSubmitting ? <LoadingIndicator /> : 'Send Coins Now'}
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
