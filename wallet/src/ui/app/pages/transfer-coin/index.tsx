// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isValidSuiAddress } from '@mysten/sui.js';
import { useFormik } from 'formik';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useIntl } from 'react-intl';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';
import * as Yup from 'yup';

import Alert from '_components/alert';
import Loading from '_components/loading';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useAppSelector, useAppDispatch } from '_hooks';
import {
    accountAggregateBalancesSelector,
    accountItemizedBalancesSelector,
} from '_redux/slices/account';
import {
    Coin,
    DEFAULT_GAS_BUDGET_FOR_TRANSFER,
    GAS_SYMBOL,
    GAS_TYPE_ARG,
} from '_redux/slices/sui-objects/Coin';
import { sendTokens } from '_redux/slices/transactions';
import { balanceFormatOptions } from '_shared/formatting';

import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';
import type { ChangeEventHandler } from 'react';

import st from './TransferCoin.module.scss';

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
            'gas-balance-check',
            'Insufficient SUI balance to cover gas fee',
            (amount, ctx) => {
                const { type, gasAggregateBalance } = ctx.parent;
                let availableGas = BigInt(gasAggregateBalance || 0);
                if (type === GAS_TYPE_ARG) {
                    availableGas -= BigInt(amount || 0);
                }
                // TODO: implement more sophisticated validation by taking
                // the splitting/merging fee into account
                return availableGas >= DEFAULT_GAS_BUDGET_FOR_TRANSFER;
            }
        )
        .test(
            'num-gas-coins-check',
            'Need at least 2 SUI coins to transfer a SUI coin',
            (amount, ctx) => {
                const { type, gasBalances } = ctx.parent;
                return (
                    type !== GAS_TYPE_ARG ||
                    (gasBalances && gasBalances.length) >= 2
                );
            }
        )
        .label('Amount'),
});
const initialValues = {
    to: '',
    amount: '',
    balance: '',
    type: '',
    gasBalance: '',
};
type FormValues = typeof initialValues;

// TODO: show out of sync when sui objects locally might be outdated
// TODO: clean/refactor
function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const coinType = useMemo(() => searchParams.get('type'), [searchParams]);
    const balances = useAppSelector(accountItemizedBalancesSelector);
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const coinBalance = useMemo(
        () => (coinType && aggregateBalances[coinType]) || null,
        [coinType, aggregateBalances]
    );
    const gasBalances = useMemo(
        () => balances[GAS_TYPE_ARG] || null,
        [balances]
    );
    const gasAggregateBalance = useMemo(
        () => aggregateBalances[GAS_TYPE_ARG] || null,
        [aggregateBalances]
    );
    const coinSymbol = useMemo(
        () => coinType && Coin.getCoinSymbol(coinType),
        [coinType]
    );
    const [sendError, setSendError] = useState<string | null>(null);
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const onHandleSubmit = useCallback(
        async (
            { to, type, amount }: FormValues,
            { resetForm }: FormikHelpers<FormValues>
        ) => {
            setSendError(null);
            try {
                const response = await dispatch(
                    sendTokens({
                        amount: BigInt(amount),
                        recipientAddress: to,
                        tokenTypeArg: type,
                    })
                ).unwrap();
                const txDigest =
                    response.EffectResponse.certificate.transactionDigest;
                resetForm();
                navigate(`/tx/${encodeURIComponent(txDigest)}`);
            } catch (e) {
                setSendError((e as SerializedError).message || null);
            }
        },
        [dispatch, navigate]
    );
    const intl = useIntl();
    const {
        handleSubmit,
        isValid,
        values: { amount, to: recipientAddress },
        handleChange,
        errors,
        touched,
        handleBlur,
        setFieldValue,
        isSubmitting,
    } = useFormik({
        validateOnMount: true,
        validationSchema,
        initialValues,
        onSubmit: onHandleSubmit,
    });
    useEffect(() => {
        setFieldValue('balance', coinBalance?.toString() || '0');
        setFieldValue('type', coinType);
        setFieldValue(
            'gasBalances',
            gasBalances?.map((b: bigint) => b.toString()) || []
        );
        setFieldValue(
            'gasAggregateBalance',
            gasAggregateBalance?.toString() || '0'
        );
    }, [
        coinBalance,
        coinType,
        gasAggregateBalance,
        gasBalances,
        setFieldValue,
    ]);
    useEffect(() => {
        setSendError(null);
    }, [amount, recipientAddress]);
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
                            value={recipientAddress}
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
                            value={amount}
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
                        * Total transaction fee estimate (gas cost):{' '}
                        {DEFAULT_GAS_BUDGET_FOR_TRANSFER} {GAS_SYMBOL}
                    </div>
                    {sendError ? (
                        <div className={st.group}>
                            <Alert>
                                <strong>Transfer failed.</strong>{' '}
                                <small>{sendError}</small>
                            </Alert>
                        </div>
                    ) : null}
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
