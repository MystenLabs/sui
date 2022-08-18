// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo } from 'react';
import { useIntl } from 'react-intl';

import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import AddressInput from '_components/address-input';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { DEFAULT_GAS_BUDGET_FOR_TRANSFER } from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import type { FormValues } from '../';

import st from './TransferCoinForm.module.scss';

export type TransferCoinFormProps = {
    submitError: string | null;
    coinBalance: string;
    coinSymbol: string;
    coinType: string;
    onClearSubmitError: () => void;
};

function StepTwo({
    submitError,
    coinBalance,
    coinSymbol,
    coinType,
    onClearSubmitError,
}: TransferCoinFormProps) {
    const {
        isSubmitting,
        isValid,
        values: { amount, to },
    } = useFormikContext<FormValues>();

    const intl = useIntl();

    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;

    useEffect(() => {
        onClearRef.current();
    }, [amount, to]);

    const totalAmount = parseFloat(amount) + DEFAULT_GAS_BUDGET_FOR_TRANSFER;

    const validAddressBtn = !isValid || to === '' || isSubmitting;

    return (
        <Form className={st.container} autoComplete="off" noValidate={true}>
            <BottomMenuLayout>
                <Content>
                    <div className={st.labelDirection}>
                        Enter or search the address of the recepient below to
                        start sending coins.
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
                            {intl.formatNumber(
                                BigInt(amount || 0),
                                balanceFormatOptions
                            )}{' '}
                            <span>{coinSymbol}</span>
                        </div>

                        <div className={st.details}>
                            <div className={st.txFees}>
                                <div className={st.txInfoLabel}>Gas Fee</div>
                                <div className={st.walletInfoValue}>
                                    {DEFAULT_GAS_BUDGET_FOR_TRANSFER}{' '}
                                    {coinSymbol}
                                </div>
                            </div>

                            <div className={st.txFees}>
                                <div className={st.txInfoLabel}>
                                    Total Amount
                                </div>
                                <div className={st.walletInfoValue}>
                                    {intl.formatNumber(
                                        BigInt(totalAmount || 0),
                                        balanceFormatOptions
                                    )}{' '}
                                    {coinSymbol}
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
                            {isSubmitting ? (
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
            </BottomMenuLayout>
        </Form>
    );
}

export default memo(StepTwo);
