// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Field, Formik, Form, ErrorMessage } from 'formik';
import { toast } from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';
import { object, string as YupString } from 'yup';

import Alert from '../../alert';
import { useBackgroundClient } from '_src/ui/app/hooks/useBackgroundClient';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { Link } from '_src/ui/app/shared/Link';
import FieldLabel from '_src/ui/app/shared/field-label';
import { Heading } from '_src/ui/app/shared/heading';
import PasswordInput from '_src/ui/app/shared/input/password';

const validation = object({
    password: YupString().ensure().required().label('Password'),
});

export type PasswordExportDialogProps = {
    title: string;
    onPasswordVerified: (password: string) => Promise<void> | void;
};

export function PasswordInputDialog({
    title,
    onPasswordVerified,
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
                            <Field name="password" component={PasswordInput} />
                            <ErrorMessage
                                render={(error) => <Alert>{error}</Alert>}
                                name="password"
                            />
                        </FieldLabel>
                    </div>
                    <div className="flex flex-col flex-nowrap gap-3.75 self-stretch">
                        <Button
                            type="submit"
                            variant="primary"
                            size="tall"
                            text="Continue"
                            loading={isSubmitting}
                            disabled={!isValid}
                        />
                        <Link
                            text="Cancel"
                            color="heroDark"
                            onClick={() => {
                                navigate(-1);
                            }}
                            disabled={isSubmitting}
                        />
                    </div>
                </Form>
            )}
        </Formik>
    );
}
