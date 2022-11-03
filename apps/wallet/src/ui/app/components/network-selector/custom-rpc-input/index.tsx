// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Field, Formik, Form } from 'formik';
import { useState, useCallback } from 'react';
import * as Yup from 'yup';

import { ENV_TO_API } from '_app/ApiProvider';
import Button from '_app/shared/button';
import InputWithAction from '_app/shared/input-with-action';
import Alert from '_components/alert';
import { useAppSelector, useAppDispatch } from '_hooks';
import { setCustomRPC } from '_redux/slices/app';

import st from '../NetworkSelector.module.scss';

const MIN_CHAR = 5;

const validation = Yup.object({
    rpcInput: Yup.string().required().min(MIN_CHAR).label('Custom RPC URL'),
});

export function CustomRPCInput() {
    const placeholder = ENV_TO_API.customRPC.fullNode;

    const customRPC = useAppSelector(({ app }) => app.customRPC);
    const [customRPCURL, setTimerMinutes] = useState<string>(customRPC || '');
    const dispatch = useAppDispatch();

    const changeNetwork = useCallback(
        async ({ rpcInput }: { rpcInput: string }) => {
            dispatch(setCustomRPC(rpcInput));
            setTimerMinutes(rpcInput);
        },
        [dispatch]
    );

    return (
        <Formik
            initialValues={{ rpcInput: customRPCURL }}
            validationSchema={validation}
            onSubmit={changeNetwork}
            enableReinitialize={false}
        >
            {({ dirty, isSubmitting, isValid, touched, errors }) => (
                <Form>
                    <Field
                        component={InputWithAction}
                        type="text"
                        name="rpcInput"
                        min={MIN_CHAR}
                        placeholder={placeholder}
                        disabled={isSubmitting}
                    >
                        <Button
                            type="submit"
                            disabled={!dirty || isSubmitting || !isValid}
                            size="mini"
                            className={st.action}
                        >
                            Save
                        </Button>
                    </Field>
                    {touched.rpcInput && errors.rpcInput ? (
                        <Alert className={st.error}>{errors.rpcInput}</Alert>
                    ) : null}
                </Form>
            )}
        </Formik>
    );
}
