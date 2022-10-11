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

import st from './StepTwo.module.scss';

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
                <Form className={st.form}>
                    <PasswordFields />
                    <div className={st.fill} />
                    <div className={st.actions}>
                        <Button
                            type="button"
                            disabled={isSubmitting}
                            className={st.btn}
                            mode="neutral"
                            size="large"
                            onClick={() => next(values, -1)}
                        >
                            <Icon
                                icon={SuiIcons.ArrowLeft}
                                className={st.prev}
                            />
                            Back
                        </Button>
                        <Button
                            type="submit"
                            disabled={isSubmitting || !isValid}
                            mode="primary"
                            className={st.btn}
                            size="large"
                        >
                            <Loading loading={isSubmitting}>
                                {mode === 'import' ? 'Import' : 'Reset'}
                                <Icon
                                    icon={SuiIcons.ArrowRight}
                                    className={st.next}
                                />
                            </Loading>
                        </Button>
                    </div>
                </Form>
            )}
        </Formik>
    );
}
