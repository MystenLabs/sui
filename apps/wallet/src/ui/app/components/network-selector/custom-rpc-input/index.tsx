// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { Field, Formik, Form } from 'formik';
import { useCallback } from 'react';
import { toast } from 'react-hot-toast';
import * as Yup from 'yup';

import Button from '_app/shared/button';
import InputWithAction from '_app/shared/input-with-action';
import Alert from '_components/alert';
import { useAppSelector, useAppDispatch } from '_hooks';
import { changeActiveNetwork } from '_redux/slices/app';
import { isValidUrl } from '_src/shared/utils';
import { API_ENV } from '_src/ui/app/ApiProvider';

import st from '../NetworkSelector.module.scss';

const MIN_CHAR = 5;

const validation = Yup.object({
    rpcInput: Yup.string()
        .required()
        .label('Custom RPC URL')
        .min(MIN_CHAR)
        .test('validate-url', 'Not a valid URL', (value) =>
            isValidUrl(value || null)
        ),
});

export function CustomRPCInput() {
    const placeholder = 'http://localhost:3000/';

    const customRPC = useAppSelector(({ app }) => app.customRPC || '');

    const dispatch = useAppDispatch();

    const changeNetwork = useCallback(
        async ({ rpcInput }: { rpcInput: string }) => {
            try {
                await dispatch(
                    changeActiveNetwork({
                        network: {
                            env: API_ENV.customRPC,
                            customRpcUrl: rpcInput,
                        },
                        store: true,
                    })
                ).unwrap();
            } catch (e) {
                toast.error((e as Error).message);
            }
        },
        [dispatch]
    );

    return (
        <Formik
            initialValues={{ rpcInput: customRPC }}
            validationSchema={validation}
            onSubmit={changeNetwork}
            enableReinitialize={true}
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
                            className={cl(
                                st.action,
                                isValid && dirty && st.canSave,
                                isValid && st.valid
                            )}
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
