// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef, memo } from 'react';

import AddressInput from '_components/address-input';
import Alert from '_components/alert';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import {
    GAS_SYMBOL,
    DEFAULT_NFT_TRANSFER_GAS_FEE,
} from '_redux/slices/sui-objects/Coin';

import type { FormValues } from '.';

import st from './TransferNFTForm.module.scss';

export type TransferNFTFormProps = {
    submitError: string | null;
    gasBalance: string;
    onClearSubmitError: () => void;
};

function TransferNFTForm({
    submitError,
    gasBalance,
    onClearSubmitError,
}: TransferNFTFormProps) {
    const {
        isSubmitting,
        isValid,
        values: { to, amount },
    } = useFormikContext<FormValues>();

    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;
    useEffect(() => {
        onClearRef.current();
    }, [to, amount]);
    return (
        <Form className={st.container} autoComplete="off" noValidate={true}>
            <div className={st.group}>
                <label className={st.label}>To:</label>
                <Field
                    component={AddressInput}
                    name="to"
                    className={st.input}
                />
                <div className={st.muted}>The recipient&apos;s address</div>
                <ErrorMessage className={st.error} name="to" component="div" />
            </div>

            <div className={st.group}>
                * Total transaction fee estimate (gas cost):
                {DEFAULT_NFT_TRANSFER_GAS_FEE} {GAS_SYMBOL}
            </div>
            {BigInt(gasBalance) < DEFAULT_NFT_TRANSFER_GAS_FEE && (
                <div className={st.error}>
                    * Insufficient balance to cover transfer cost
                </div>
            )}
            {submitError ? (
                <div className={st.group}>
                    <Alert>
                        <strong>Transfer failed.</strong>{' '}
                        <small>{submitError}</small>
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
        </Form>
    );
}

export default memo(TransferNFTForm);
