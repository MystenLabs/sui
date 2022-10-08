// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Formik, Form, Field } from 'formik';
import { useNavigate } from 'react-router-dom';

import { createMnemonicValidation } from './validation';
import Button from '_app/shared/button';
import PasswordInput from '_app/shared/input/password';
import { WALLET_ENCRYPTION_ENABLED } from '_app/wallet/constants';
import Alert from '_components/alert';
import ExternalLink from '_components/external-link';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { useAppDispatch } from '_hooks';
import CardLayout from '_pages/initialize/shared/card-layout';
import { createMnemonic } from '_redux/slices/account';
import { PRIVACY_POLICY_LINK, ToS_LINK } from '_shared/constants';

import st from './Create.module.scss';

const PASSWORD_INFO_ERROR =
    'Minimum 8 characters. Password must include at least one number and uppercase letter.';

const CreatePage = () => {
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const headerObj = {
        [WALLET_ENCRYPTION_ENABLED ? 'headerCaption' : 'title']:
            'Create New wallet',
    };
    return (
        <CardLayout title="Create Password for This Wallet" {...headerObj}>
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
                            createMnemonic({ password: values['password'] })
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
                                {WALLET_ENCRYPTION_ENABLED ? (
                                    <>
                                        <label className={st.label}>
                                            <span className={st.labelText}>
                                                Create Password
                                            </span>
                                            <Field
                                                name="password"
                                                component={PasswordInput}
                                            />
                                            {touched['password'] &&
                                            errors['password'] ? (
                                                <Alert>
                                                    {PASSWORD_INFO_ERROR}
                                                </Alert>
                                            ) : (
                                                <div className={st.info}>
                                                    {PASSWORD_INFO_ERROR}
                                                </div>
                                            )}
                                        </label>
                                        <label className={st.label}>
                                            <span className={st.labelText}>
                                                Confirm Password
                                            </span>
                                            <Field
                                                name="confirmPassword"
                                                component={PasswordInput}
                                            />
                                            {touched['confirmPassword'] &&
                                            errors['confirmPassword'] ? (
                                                <Alert>
                                                    {errors['confirmPassword']}
                                                </Alert>
                                            ) : null}
                                        </label>
                                    </>
                                ) : (
                                    <>
                                        <div className={st.space} />
                                        <div className={st.desc}>
                                            Creating a wallet generates new
                                            recovery passphrase. Using it you
                                            can backup and restore your wallet.
                                        </div>
                                    </>
                                )}
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
