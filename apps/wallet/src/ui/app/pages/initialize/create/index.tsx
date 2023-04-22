// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16, Check12 } from '@mysten/icons';
import { Formik, Form, Field } from 'formik';
import { useNavigate } from 'react-router-dom';

import { createMnemonicValidation } from './validation';
import { Button } from '_app/shared/ButtonUI';
import { CardLayout } from '_app/shared/card-layout';
import { Text } from '_app/shared/text';
import ExternalLink from '_components/external-link';
import { useAppDispatch } from '_hooks';
import PasswordFields from '_pages/initialize/shared/password-fields';
import { createVault } from '_redux/slices/account';
import { ToS_LINK } from '_shared/constants';

const CreatePage = () => {
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    return (
        <CardLayout
            title="Create Password for This Wallet"
            headerCaption="create a new wallet"
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
                {({ isValid, isSubmitting }) => (
                    <Form className="flex flex-col flex-nowrap mt-7.5 flex-grow w-full">
                        <div className="flex flex-col flex-nowrap flex-grow">
                            <fieldset
                                disabled={isSubmitting}
                                className="contents"
                            >
                                <PasswordFields />
                                <div className="flex-1" />
                                <label className="flex items-center justify-center h-5 my-5 text-gray-75 gap-1.25 relative cursor-pointer">
                                    <Field
                                        name="terms"
                                        type="checkbox"
                                        id="terms"
                                        className="peer/terms invisible"
                                    />
                                    <span className="absolute top-0 left-0.5 h-5 w-5 bg-white peer-checked/terms:bg-success peer-checked/terms:shadow-none  border-gray-50 border rounded shadow-button flex justify-center items-center">
                                        <Check12 className="text-white text-body font-semibold" />
                                    </span>
                                    <Text
                                        variant="bodySmall"
                                        color="steel-dark"
                                        weight="normal"
                                    >
                                        I read and agreed to the{' '}
                                        <ExternalLink
                                            href={ToS_LINK}
                                            className="text-[#1F6493] no-underline"
                                        >
                                            Terms of Services
                                        </ExternalLink>
                                    </Text>
                                </label>
                            </fieldset>
                        </div>
                        <Button
                            type="submit"
                            disabled={!isValid || isSubmitting}
                            size="tall"
                            text="Create Wallet"
                            loading={isSubmitting}
                            after={<ArrowRight16 />}
                        />
                    </Form>
                )}
            </Formik>
        </CardLayout>
    );
};

export default CreatePage;
