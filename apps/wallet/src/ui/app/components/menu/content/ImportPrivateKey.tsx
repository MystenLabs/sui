// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16 } from '@mysten/icons';
import { type ExportedKeypair, toB64 } from '@mysten/sui.js';
import { hexToBytes } from '@noble/hashes/utils';
import { useMutation } from '@tanstack/react-query';
import { ErrorMessage, Field, Form, Formik } from 'formik';
import { useState } from 'react';
import { toast } from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';
import { object, string as yupString } from 'yup';

import Alert from '../../alert';
import { useNextMenuUrl } from '../hooks';
import { MenuLayout } from './MenuLayout';
import { PasswordInputDialog } from './PasswordInputDialog';
import { useBackgroundClient } from '_src/ui/app/hooks/useBackgroundClient';
import { Button } from '_src/ui/app/shared/ButtonUI';
import FieldLabel from '_src/ui/app/shared/field-label';

const validation = object({
    privateKey: yupString()
        .ensure()
        .trim()
        .required()
        .transform((value: string) => {
            if (value.startsWith('0x')) {
                return value.substring(2);
            }
            return value;
        })
        .test(
            'valid-hex',
            `\${path} must be a hexadecimal value. It may optionally begin with "0x".`,
            (value: string) => {
                try {
                    hexToBytes(value);
                    return true;
                } catch (e) {
                    return false;
                }
            }
        )
        .test(
            'valid-bytes-length',
            `\${path} must be either 32 or 64 bytes.`,
            (value: string) => {
                try {
                    const bytes = hexToBytes(value);
                    return [32, 64].includes(bytes.length);
                } catch (e) {
                    return false;
                }
            }
        )
        .label('Private Key'),
});

export function ImportPrivateKey() {
    const accountsUrl = useNextMenuUrl(true, `/accounts`);
    const backgroundClient = useBackgroundClient();
    const navigate = useNavigate();
    const [showPasswordDialog, setShowPasswordDialog] = useState(false);
    const [privateKey, setPrivateKey] = useState('');
    const importMutation = useMutation({
        mutationFn: async (password: string) => {
            const keyPair: ExportedKeypair = {
                schema: 'ED25519',
                privateKey: toB64(hexToBytes(privateKey)),
            };
            await backgroundClient.importPrivateKey(password, keyPair);
        },
        onSuccess: () => {
            toast.success('Account imported');
            navigate(accountsUrl);
        },
        onError: () => setShowPasswordDialog(false),
    });
    return showPasswordDialog ? (
        <div className="absolute inset-0 pb-8 px-2.5 flex flex-col z-10">
            <PasswordInputDialog
                title="Import Account"
                continueLabel="Import"
                onBackClicked={() => setShowPasswordDialog(false)}
                onPasswordVerified={async (password) => {
                    await importMutation.mutateAsync(password);
                }}
            />
        </div>
    ) : (
        <MenuLayout title="Import Existing Account" back={accountsUrl}>
            <Formik
                initialValues={{ privateKey }}
                onSubmit={async ({ privateKey: privateKeyInput }) => {
                    setPrivateKey(
                        validation.cast({ privateKey: privateKeyInput })
                            .privateKey
                    );
                    setShowPasswordDialog(true);
                }}
                validationSchema={validation}
                validateOnMount
                enableReinitialize
            >
                {({ isSubmitting, isValid }) => (
                    <Form className="flex flex-col gap-3 pt-2.5">
                        <FieldLabel txt="Enter Private Key">
                            <Field
                                name="privateKey"
                                className="shadow-button text-steel-dark font-medium text-pBody resize-none rounded-xl border border-solid border-steel p-3"
                                component={'textarea'}
                                rows="3"
                                spellCheck="false"
                                autoComplete="off"
                                autoFocus
                            />
                            <ErrorMessage
                                render={(error) => <Alert>{error}</Alert>}
                                name="privateKey"
                            />
                        </FieldLabel>
                        <Button
                            type="submit"
                            size="tall"
                            variant="primary"
                            text="Continue"
                            after={<ArrowRight16 />}
                            disabled={!isValid}
                            loading={isSubmitting}
                        />
                    </Form>
                )}
            </Formik>
        </MenuLayout>
    );
}
