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
import { mnemonicValidation } from '_pages/initialize/import/validation';

import type { StepProps } from '.';

import st from './StepOne.module.scss';

const validationSchema = Yup.object({
    mnemonic: mnemonicValidation,
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
                values: { mnemonic },
                isValid,
                handleChange,
                setFieldValue,
                handleBlur,
            }) => (
                <Form className={st.form}>
                    <FieldLabel txt="Enter Recovery Phrase">
                        <textarea
                            id="importMnemonicTxt"
                            onChange={handleChange}
                            value={mnemonic}
                            onBlur={async (e) => {
                                const adjMnemonic =
                                    await validationSchema.fields.mnemonic.cast(
                                        mnemonic
                                    );
                                await setFieldValue(
                                    'mnemonic',
                                    adjMnemonic,
                                    false
                                );
                                handleBlur(e);
                            }}
                            className={st.mnemonic}
                            placeholder="Enter your 12-word recovery phrase"
                            name="mnemonic"
                            disabled={isSubmitting}
                        />
                        {touched.mnemonic && errors?.mnemonic && (
                            <Alert>{errors?.mnemonic}</Alert>
                        )}
                    </FieldLabel>
                    <div className={st.fill} />
                    <div className={st.actionsContainer}>
                        {mode === 'forgot' ? (
                            <Button
                                type="button"
                                disabled={isSubmitting}
                                mode="neutral"
                                size="large"
                                className={st.btn}
                                onClick={() => {
                                    navigate(-1);
                                }}
                            >
                                <Icon
                                    icon={SuiIcons.ArrowLeft}
                                    className={st.btnIcon}
                                />
                                Back
                            </Button>
                        ) : null}
                        <Button
                            type="submit"
                            disabled={isSubmitting || !isValid}
                            mode="primary"
                            className={st.btn}
                            size="large"
                        >
                            <Loading loading={isSubmitting}>
                                {mode === 'forgot' ? 'Next' : 'Continue'}
                                <Icon
                                    icon={SuiIcons.ArrowRight}
                                    className={st.btnIcon}
                                />
                            </Loading>
                        </Button>
                    </div>
                </Form>
            )}
        </Formik>
    );
}
