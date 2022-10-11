// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import StepOne from './steps/StepOne';
import StepTwo from './steps/StepTwo';
import CardLayout from '_app/shared/card-layout';
import { WALLET_ENCRYPTION_ENABLED } from '_app/wallet/constants';
import { useAppDispatch } from '_hooks';
import { createMnemonic, logout } from '_redux/slices/account';
import { MAIN_UI_URL } from '_src/shared/utils';

const initialValues = {
    mnemonic: '',
    password: '',
    confirmPassword: '',
};

const allSteps = [StepOne];

if (WALLET_ENCRYPTION_ENABLED) {
    allSteps.push(StepTwo);
}

export type ImportValuesType = typeof initialValues;
export type ImportPageProps = {
    mode?: 'import' | 'forgot';
};
const ImportPage = ({ mode = 'import' }: ImportPageProps) => {
    const [data, setData] = useState<ImportValuesType>(initialValues);
    const [step, setStep] = useState(0);
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const onHandleSubmit = useCallback(
        async ({ mnemonic, password }: ImportValuesType) => {
            try {
                if (mode === 'forgot') {
                    // clear everything in storage
                    await dispatch(logout());
                }
                await dispatch(
                    createMnemonic({ importedMnemonic: mnemonic, password })
                ).unwrap();
                if (mode === 'import') {
                    navigate('../backup-imported');
                } else {
                    // refresh the page to re-initialize the store
                    window.location.href = MAIN_UI_URL;
                }
            } catch (e) {
                // Do nothing
            }
        },
        [dispatch, navigate, mode]
    );
    const totalSteps = allSteps.length;
    const StepForm = step < totalSteps ? allSteps[step] : null;
    return (
        <CardLayout
            title={
                mode === 'import'
                    ? 'Import an Existing Wallet'
                    : 'Reset Password for This Wallet'
            }
            headerCaption={mode === 'import' ? 'Wallet Setup' : undefined}
            mode={mode === 'import' ? 'box' : 'plain'}
            goBackOnClick={
                mode === 'forgot'
                    ? () => {
                          if (step > 0) {
                              setStep((step) => step - 1);
                          } else {
                              navigate(-1);
                          }
                      }
                    : undefined
            }
        >
            {StepForm ? (
                <StepForm
                    next={async (data, stepIncrement) => {
                        const nextStep = step + stepIncrement;
                        if (nextStep >= totalSteps) {
                            await onHandleSubmit(data);
                        }
                        setData(data);
                        if (nextStep < 0) {
                            return;
                        }
                        setStep(nextStep);
                    }}
                    data={data}
                    mode={mode}
                />
            ) : null}
        </CardLayout>
    );
};

export default ImportPage;
