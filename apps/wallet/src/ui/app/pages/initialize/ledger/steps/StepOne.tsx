// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Formik, Form } from 'formik';
import { useNavigate } from 'react-router-dom';
import * as Yup from 'yup';

import Button from '_app/shared/button';
import FieldLabel from '_app/shared/field-label';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { derivationPathValidation } from '_pages/initialize/ledger/validation';

import type { StepProps } from '.';

const validationSchema = Yup.object({
    derivationPath: derivationPathValidation,
});

export default function StepOne({ next, data, mode }: StepProps) {
    const navigate = useNavigate();
    return (
        <Formik
            initialValues={data}
            validationSchema={validationSchema}
            validateOnMount={true}
            onSubmit={async (values) => {
                await next(values, 1);
            }}
            enableReinitialize={true}
        >
            {({
                isSubmitting,
                touched,
                errors,
                values: { derivationPath },
                isValid,
                handleChange,
                setFieldValue,
                handleBlur,
            }) => (
                <Form className="flex flex-col flex-nowrap items-stretch flex-1 flex-grow justify-between">
                    <FieldLabel txt="Enter Derivation Path">
                        <textarea
                            id="importMnemonicTxt"
                            onChange={handleChange}
                            value={derivationPath}
                            onBlur={async (e) => {
                                await setFieldValue(
                                    'derivationPath',
                                    (x: string) => x,
                                    false
                                );
                                handleBlur(e);
                            }}
                            className="text-steel-dark flex flex-col flex-nowrap gap-2 self-stretch font-semibold text-heading5 p-3.5 rounded-15 bg-white border border-solid border-gray-45 shadow-button leading-snug resize-none min-h-[100px] placeholder:text-steel-dark"
                            placeholder="Enter your derivation path (m/44'/784'/0'/0'/0' is fine default)"
                            name="derivationPath"
                            disabled={isSubmitting}
                        />
                        {touched.derivationPath && errors?.derivationPath && (
                            <Alert>{errors?.derivationPath}</Alert>
                        )}
                    </FieldLabel>

                    <div className="flex flex-nowrap items-center mt-5 gap-2.5">
                        {mode === 'forgot' ? (
                            <Button
                                type="button"
                                disabled={isSubmitting}
                                mode="neutral"
                                size="large"
                                className="flex-1"
                                onClick={() => {
                                    navigate(-1);
                                }}
                            >
                                <Icon
                                    icon={SuiIcons.ArrowLeft}
                                    className="text-subtitleSmallExtra font-light"
                                />
                                Back
                            </Button>
                        ) : null}
                        <Button
                            type="submit"
                            disabled={isSubmitting || !isValid}
                            mode="primary"
                            className="flex-1"
                            size="large"
                        >
                            <Loading loading={isSubmitting}>
                                {mode === 'forgot' ? 'Next' : 'Continue'}
                                <Icon
                                    icon={SuiIcons.ArrowRight}
                                    className="text-subtitleSmallExtra font-light"
                                />
                            </Loading>
                        </Button>
                    </div>
                </Form>
            )}
        </Formik>
    );
}
