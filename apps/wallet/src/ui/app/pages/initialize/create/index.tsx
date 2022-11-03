// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Formik, Form, Field } from 'formik';
import { useNavigate } from 'react-router-dom';

import { createMnemonicValidation } from './validation';
import Button from '_app/shared/button';
import CardLayout from '_app/shared/card-layout';
import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { useAppDispatch } from '_hooks';
import PasswordFields from '_pages/initialize/shared/password-fields';
import { createVault } from '_redux/slices/account';
import { PRIVACY_POLICY_LINK, ToS_LINK } from '_shared/constants';

import st from './Create.module.scss';

const CreatePage = () => {
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    return (
        <CardLayout
            title="Create Password for This Wallet"
            headerCaption="Create New wallet"
        >
            <Formik
                initialValues={{
                    terms: false,
                    password: '',
                    confirmPassword: '',
                }}
                validationSchema={createMnemonicValidation}
                validateOnMount={true}
                onSubmit={async (values) => {
                    try {
                        await dispatch(
                            createVault({ password: values.password })
                        ).unwrap();
                        navigate('../backup');
                    } catch (e) {
                        // Do nothing
                    }
                }}
            >
                {({ isValid, isSubmitting, errors, touched }) => (
                    <Form className={st.matchParent}>
                        <div className={st.matchParent}>
                            <fieldset
                                disabled={isSubmitting}
                                className={st.fieldset}
                            >
                                <PasswordFields />
                                <div className={st.space} />
                                <label className={st.terms}>
                                    <Field name="terms" type="checkbox" />
                                    <span className={st.checkBox}></span>
                                    <span className={st.checkboxLabel}>
                                        I read and agreed to the{' '}
                                        <ExternalLink
                                            href={ToS_LINK}
                                            showIcon={false}
                                        >
                                            Terms of Service
                                        </ExternalLink>{' '}
                                        and the{' '}
                                        <ExternalLink
                                            href={PRIVACY_POLICY_LINK}
                                            showIcon={false}
                                        >
                                            Privacy Policy
                                        </ExternalLink>
                                        .
                                    </span>
                                </label>
                            </fieldset>
                        </div>
                        <Button
                            type="submit"
                            disabled={!isValid || isSubmitting}
                            mode="primary"
                            size="large"
                        >
                            <Loading loading={isSubmitting}>
                                Create Wallet
                                <Icon
                                    icon={SuiIcons.ArrowRight}
                                    className={st.next}
                                />
                            </Loading>
                        </Button>
                    </Form>
                )}
            </Formik>
        </CardLayout>
    );
};

export default CreatePage;
