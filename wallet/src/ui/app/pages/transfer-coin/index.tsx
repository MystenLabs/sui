// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isValidSuiAddress } from '@mysten/sui.js';
import { useFormik } from 'formik';
import { useCallback, useEffect, useMemo } from 'react';
import { useIntl } from 'react-intl';
import { Navigate, useSearchParams } from 'react-router-dom';
import * as Yup from 'yup';

import Loading from '_components/loading';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useAppSelector } from '_hooks';
import { accountBalancesSelector } from '_redux/slices/account';
import { Coin, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import type { ChangeEventHandler } from 'react';

import st from './TransferCoin.module.scss';

// TODO: calculate the transfer gas fee
const GAS_FEE = 100;
const addressValidation = Yup.string()
    .ensure()
    .trim()
    .required()
    .transform((value: string) =>
        value.startsWith('0x') || value === '' || value === '0'
            ? value
            : `0x${value}`
    )
    // eslint-disable-next-line no-template-curly-in-string
    .test('is-sui-address', '${value} is not a valid Sui address', (value) =>
        isValidSuiAddress(value)
    )
    .label("Recipient's address");
const validationSchema = Yup.object({
    to: addressValidation,
    amount: Yup.number()
        .required()
        .integer()
        .min(1)
        .max(Yup.ref('balance'))
        .test(
            'available-gas-check',
            'Insufficient funds for gas',
            (amount, ctx) => {
                const { type, gasBalance } = ctx.parent;
                let availableGas = BigInt(gasBalance || 0);
                if (type === GAS_TYPE_ARG) {
                    availableGas -= BigInt(amount || 0);
                }
                return availableGas >= GAS_FEE;
            }
        )
        .label('Amount'),
});

// TODO: show out of sync when sui objects locally might be outdated
// TODO: clean/refactor
function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const coinType = useMemo(() => searchParams.get('type'), [searchParams]);
    const balances = useAppSelector(accountBalancesSelector);
    const coinBalance = useMemo(
        () => (coinType && balances[coinType]) || null,
        [coinType, balances]
    );
    const gasBalance = useMemo(
        () => balances[GAS_TYPE_ARG] || null,
        [balances]
    );
    const coinSymbol = useMemo(
        () => coinType && Coin.getCoinSymbol(coinType),
        [coinType]
    );
    const intl = useIntl();
    const {
        handleSubmit,
        isValid,
        values,
        handleChange,
        errors,
        touched,
        handleBlur,
        setFieldValue,
        isSubmitting,
    } = useFormik({
        validateOnMount: true,
        validationSchema,
        initialValues: {
            to: '',
            amount: '',
            balance: '',
            type: '',
            gasBalance: '',
        },
        onSubmit: (values) => {
            // TODO: execute transaction and show result
            return new Promise((r) => setTimeout(r, 5000));
        },
    });
    useEffect(() => {
        setFieldValue('balance', coinBalance?.toString() || '0');
        setFieldValue('type', coinType);
        setFieldValue('gasBalance', gasBalance?.toString() || '0');
    }, [coinBalance, coinType, gasBalance, setFieldValue]);
    const loadingBalance = useAppSelector(
        ({ suiObjects }) => suiObjects.loading && !suiObjects.lastSync
    );
    const handleAddressOnChange = useCallback<
        ChangeEventHandler<HTMLInputElement>
    >(
        (e) => {
            const address = e.currentTarget.value;
            setFieldValue('to', addressValidation.cast(address));
        },
        [setFieldValue]
    );
    if (!coinType) {
        return <Navigate to="/" replace={true} />;
    }
    return (
        <>
            <h3>Send {coinSymbol}</h3>
            <Loading loading={loadingBalance}>
                <form
                    className={st.container}
                    onSubmit={handleSubmit}
                    autoComplete="off"
                    noValidate={true}
                >
                    <div className={st.group}>
                        <label className={st.label}>To:</label>
                        <input
                            value={values.to}
                            onChange={handleAddressOnChange}
                            onBlur={handleBlur}
                            name="to"
                            className={st.input}
                            placeholder="0x..."
                            disabled={isSubmitting}
                        />
                        <span className={st.muted}>
                            The recipient&apos;s address
                        </span>
                        <span className={st.error}>
                            {(touched.to && errors.to) || null}
                        </span>
                    </div>
                    <div className={st.group}>
                        <label className={st.label}>Amount:</label>
                        <input
                            type="number"
                            step="1"
                            min={0}
                            max={coinBalance?.toString() || 0}
                            value={values.amount}
                            name="amount"
                            onChange={handleChange}
                            onBlur={handleBlur}
                            placeholder={`Total ${coinSymbol?.toLocaleUpperCase()} to send`}
                            className={st.input}
                            disabled={isSubmitting}
                        />
                        <span className={st.muted}>
                            Available balance:{' '}
                            {intl.formatNumber(
                                coinBalance || 0,
                                balanceFormatOptions
                            )}{' '}
                            {coinSymbol}
                        </span>
                        <span className={st.error}>
                            {(touched.amount && errors.amount) || null}
                        </span>
                    </div>
                    <div className={st.group}>
                        * Total transaction fee estimate (gas cost): {GAS_FEE}{' '}
                        SUI
                    </div>
                    <div className={st.group}>
                        <button
                            type="submit"
                            disabled={!isValid || isSubmitting}
                            className="btn"
                        >
                            {isSubmitting ? <LoadingIndicator /> : 'Send'}
                        </button>
                    </div>
                </form>
            </Loading>
        </>
    );
}

export default TransferCoinPage;
