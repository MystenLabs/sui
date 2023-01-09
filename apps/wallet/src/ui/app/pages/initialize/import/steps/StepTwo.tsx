// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Form, Formik } from 'formik';
import { object } from 'yup';

import Button from '_app/shared/button';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import PasswordFields from '_pages/initialize/shared/password-fields';
import { passwordFieldsValidation } from '_pages/initialize/shared/password-fields/validation';

import type { StepProps } from '.';

const validationSchema = object(passwordFieldsValidation);

export default function StepTwo({ next, data, mode }: StepProps) {
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
            {({ isSubmitting, isValid, values }) => (
                <Form className="flex flex-col flex-nowrap self-stretch flex-1">
                    <PasswordFields />
                    <div className="flex-1" />
                    <div className="flex flex-nowrap gap-2.5">
                        <Button
                            type="button"
                            disabled={isSubmitting}
                            className="flex-1 !text-steel-dark"
                            mode="neutral"
                            size="large"
                            onClick={() => next(values, -1)}
                        >
                            <Icon
                                icon={SuiIcons.ArrowLeft}
                                className="text-subtitleSmallExtra font-normal"
                            />
                            Back
                        </Button>
                        <Button
                            type="submit"
                            disabled={isSubmitting || !isValid}
                            mode="primary"
                            className="flex-1"
                            size="large"
                        >
                            <Loading loading={isSubmitting}>
                                {mode === 'import' ? 'Import' : 'Reset'}
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
