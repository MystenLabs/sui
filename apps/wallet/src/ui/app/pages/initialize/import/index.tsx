// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormik } from 'formik';
import { useCallback } from 'react';
import * as Yup from 'yup';

import Button from '_app/shared/button';
import Icon, { SuiIcons } from '_components/icon';
import { useAppDispatch, useAppSelector } from '_hooks';
import { createMnemonic, setMnemonic } from '_redux/slices/account';
import { normalizeMnemonics, validateMnemonics } from '_src/shared/utils/bip39';

import type { FocusEventHandler } from 'react';

import st from './Import.module.scss';

const validationSchema = Yup.object({
    mnemonic: Yup.string()
        .ensure()
        .required()
        .trim()
        .transform((mnemonic) => normalizeMnemonics(mnemonic))
        .test('mnemonic-valid', 'Recovery Passphrase is invalid', (mnemonic) =>
            validateMnemonics(mnemonic)
        )
        .label('Recovery Passphrase'),
});

const initialValues = {
    mnemonic: '',
};
type ValuesType = typeof initialValues;

const ImportPage = () => {
    const createInProgress = useAppSelector(({ account }) => account.creating);
    const dispatch = useAppDispatch();
    const onHandleSubmit = useCallback(
        async ({ mnemonic }: ValuesType) => {
            await dispatch(createMnemonic(mnemonic));
            await dispatch(setMnemonic(mnemonic));
        },
        [dispatch]
    );
    const {
        handleBlur,
        handleChange,
        values: { mnemonic },
        isSubmitting,
        isValid,
        errors,
        touched,
        handleSubmit,
        setFieldValue,
    } = useFormik({
        initialValues,
        onSubmit: onHandleSubmit,
        validationSchema,
        validateOnMount: true,
    });
    const onHandleMnemonicBlur = useCallback<
        FocusEventHandler<HTMLTextAreaElement>
    >(
        async (e) => {
            const adjMnemonic = await validationSchema.fields.mnemonic.cast(
                mnemonic
            );
            await setFieldValue('mnemonic', adjMnemonic, false);
            handleBlur(e);
        },
        [setFieldValue, mnemonic, handleBlur]
    );
    return (
        <>
            <h1 className={st.headerTitle}>Import wallet</h1>
            <form onSubmit={handleSubmit} noValidate autoComplete="off">
                <textarea
                    onChange={handleChange}
                    value={mnemonic}
                    onBlur={onHandleMnemonicBlur}
                    className={st.mnemonic}
                    placeholder="Paste your 12-word passphrase"
                    name="mnemonic"
                    disabled={createInProgress || isSubmitting}
                />
                {touched.mnemonic && errors?.mnemonic && (
                    <div className={st.error}>{errors?.mnemonic}</div>
                )}

                <Button
                    type="submit"
                    disabled={isSubmitting || createInProgress || !isValid}
                    mode="primary"
                    className={st.btn}
                    size="large"
                >
                    Import wallet Now
                    <Icon icon={SuiIcons.ArrowRight} className={st.next} />
                </Button>
            </form>
        </>
    );
};

export default ImportPage;
