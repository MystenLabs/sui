// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Field, Form, Formik } from 'formik';
import { useNavigate, useSearchParams } from 'react-router-dom';
import * as Yup from 'yup';

import Alert from '_app/components/alert';
import Icon, { SuiIcons } from '_app/components/icon';
import Button from '_app/shared/button';
import CardLayout from '_app/shared/card-layout';
import FieldLabel from '_app/shared/field-label';
import PasswordInput from '_app/shared/input/password';
import PageMainLayout from '_app/shared/page-main-layout';
import { unlockWallet } from '_app/wallet/actions';
import { useLockedGuard } from '_app/wallet/hooks';
import Loading from '_components/loading';
import { useAppDispatch, useInitializedGuard } from '_hooks';
import PageLayout from '_pages/layout';

import st from './LockedPage.module.scss';

const validation = Yup.object({
    password: Yup.string().ensure().required('Required'),
});

export default function LockedPage() {
    const initGuardLoading = useInitializedGuard(true);
    const lockedGuardLoading = useLockedGuard(true);
    const guardsLoading = initGuardLoading || lockedGuardLoading;
    const [searchParams] = useSearchParams();
    const navigate = useNavigate();
    const dispatch = useAppDispatch();
    const nextUrl = searchParams.get('url') || '/';
    return (
        <Loading loading={guardsLoading}>
            <PageLayout limitToPopUpSize={true}>
                <PageMainLayout className={st.main}>
                    <CardLayout
                        icon="sui"
                        headerCaption="Hello There"
                        title="Welcome Back"
                        mode="plain"
                    >
                        <Formik
                            initialValues={{ password: '' }}
                            validationSchema={validation}
                            validateOnMount={true}
                            onSubmit={async (
                                { password },
                                { setFieldError }
                            ) => {
                                try {
                                    await dispatch(
                                        unlockWallet({ password })
                                    ).unwrap();
                                    navigate(nextUrl);
                                } catch (e) {
                                    setFieldError(
                                        'password',
                                        (e as Error).message ||
                                            'Incorrect password'
                                    );
                                }
                            }}
                        >
                            {({ touched, errors, isSubmitting, isValid }) => (
                                <Form className={st.form}>
                                    <FieldLabel txt="Enter Password">
                                        <Field
                                            name="password"
                                            component={PasswordInput}
                                            disabled={isSubmitting}
                                        />
                                        {touched.password && errors.password ? (
                                            <Alert>{errors.password}</Alert>
                                        ) : null}
                                    </FieldLabel>
                                    <div className={st.fill} />
                                    <Button
                                        type="submit"
                                        disabled={isSubmitting || !isValid}
                                        mode="primary"
                                        size="large"
                                    >
                                        <Icon
                                            icon={SuiIcons.CheckFill}
                                            className={st.btnIcon}
                                        />
                                        Unlock
                                    </Button>
                                </Form>
                            )}
                        </Formik>
                    </CardLayout>
                </PageMainLayout>
            </PageLayout>
        </Loading>
    );
}
