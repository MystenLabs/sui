// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';
import cl from 'classnames';
import { Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo, useMemo } from 'react';

import { Content, Menu } from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import AddressInput from '_components/address-input';
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
    gasBudget: number;
    onClearSubmitError: () => void;
};

function StepTwo({
    submitError,
    coinSymbol,
    coinType,
    gasBudget,
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

    const [decimals] = useCoinDecimals(coinType);
    const amountWithoutDecimals = useMemo(
        () =>
            new BigNumber(amount).shiftedBy(decimals).integerValue().toString(),
        [amount, decimals]
    );

    const totalAmount = new BigNumber(gasBudget)
        .plus(GAS_SYMBOL === coinSymbol ? amountWithoutDecimals : 0)
        .toString();

    const validAddressBtn = !isValid || to === '' || isSubmitting;

    const [formattedBalance] = useFormatCoin(amountWithoutDecimals, coinType);
    const [formattedTotal] = useFormatCoin(totalAmount, GAS_TYPE_ARG);
    const [formattedGas] = useFormatCoin(gasBudget, GAS_TYPE_ARG);

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
                        {formattedBalance} <span>{coinSymbol}</span>
                    </div>

                    <div className={st.details}>
                        <div className={st.txFees}>
                            <div className={st.txInfoLabel}>Gas Fee</div>
                            <div className={st.walletInfoValue}>
                                {formattedGas} {GAS_SYMBOL}
                            </div>
                        </div>

                        <div className={st.txFees}>
                            <div className={st.txInfoLabel}>Total Amount</div>
                            <div className={st.walletInfoValue}>
                                {formattedTotal} {GAS_SYMBOL}
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
