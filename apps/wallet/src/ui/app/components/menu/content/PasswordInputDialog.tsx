// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16 } from '@mysten/icons';
import { Formik, Form, ErrorMessage } from 'formik';
import { toast } from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';
import { object, string as YupString } from 'yup';

import Alert from '../../alert';
import { useBackgroundClient } from '_src/ui/app/hooks/useBackgroundClient';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { Link } from '_src/ui/app/shared/Link';
import FieldLabel from '_src/ui/app/shared/field-label';
import { Heading } from '_src/ui/app/shared/heading';
import { PasswordInputField } from '_src/ui/app/shared/input/password';
import { Text } from '_src/ui/app/shared/text';

const validation = object({
    password: YupString().ensure().required().label('Password'),
});

export type PasswordExportDialogProps = {
    title: string;
    continueLabel?: string;
    showArrowIcon?: boolean;
    onPasswordVerified: (password: string) => Promise<void> | void;
    onBackClicked?: () => void;
};

export function PasswordInputDialog({
    title,
    continueLabel = 'Continue',
    showArrowIcon = false,
    onPasswordVerified,
    onBackClicked,
}: PasswordExportDialogProps) {
    const navigate = useNavigate();
    const backgroundService = useBackgroundClient();
    return (
        <Formik
            initialValues={{ password: '' }}
            onSubmit={async ({ password }, { setFieldError }) => {
                try {
                    await backgroundService.verifyPassword(password);
                    try {
                        await onPasswordVerified(password);
                    } catch (e) {
                        toast.error((e as Error).message || 'Wrong password');
                    }
                } catch (e) {
                    setFieldError(
                        'password',
                        (e as Error).message || 'Wrong password'
                    );
                }
            }}
            validationSchema={validation}
            validateOnMount
        >
            {({ isSubmitting, isValid }) => (
                <Form className="bg-white px-5 pt-10 flex flex-col flex-nowrap items-center flex-1 gap-7.5">
                    <Heading variant="heading1" color="gray-90" weight="bold">
                        {title}
                    </Heading>
                    <div className="self-stretch flex-1">
                        <FieldLabel txt="Enter Wallet Password to Continue">
                            <PasswordInputField name="password" />
                            <ErrorMessage
                                render={(error) => <Alert>{error}</Alert>}
                                name="password"
                            />
                        </FieldLabel>
                        <div className="text-center mt-4">
                            <Text
                                variant="pBodySmall"
                                color="steel-dark"
                                weight="normal"
                            >
                                This is the password you currently use to lock
                                and unlock your Sui wallet.
                            </Text>
                        </div>
                    </div>
                    <div className="flex flex-col flex-nowrap gap-3.75 self-stretch">
                        <Button
                            type="submit"
                            variant="primary"
                            size="tall"
                            text={continueLabel}
                            loading={isSubmitting}
                            disabled={!isValid}
                            after={showArrowIcon ? <ArrowRight16 /> : null}
                        />
                        <Link
                            text="Go Back"
                            color="heroDark"
                            onClick={() => {
                                if (typeof onBackClicked === 'function') {
                                    onBackClicked();
                                } else {
                                    navigate(-1);
                                }
                            }}
                            disabled={isSubmitting}
                        />
                    </div>
                </Form>
            )}
        </Formik>
    );
}
