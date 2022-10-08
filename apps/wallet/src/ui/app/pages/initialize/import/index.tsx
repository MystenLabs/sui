// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import StepOne from './steps/StepOne';
import StepTwo from './steps/StepTwo';
import { WALLET_ENCRYPTION_ENABLED } from '_app/wallet/constants';
import { useAppDispatch } from '_hooks';
import CardLayout from '_pages/initialize/shared/card-layout';
import { createMnemonic } from '_redux/slices/account';

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

const ImportPage = () => {
    const [data, setData] = useState<ImportValuesType>(initialValues);
    const [step, setStep] = useState(0);
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const onHandleSubmit = useCallback(
        async ({ mnemonic, password }: ImportValuesType) => {
            try {
                await dispatch(
                    createMnemonic({ importedMnemonic: mnemonic, password })
                ).unwrap();
                navigate('../backup-imported');
            } catch (e) {
                // Do nothing
            }
        },
        [dispatch, navigate]
    );
    const totalSteps = allSteps.length;
    const StepForm = step < totalSteps ? allSteps[step] : null;
    return (
        <CardLayout
            title="Import an Existing Wallet"
            headerCaption="Wallet Setup"
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
                />
            ) : null}
        </CardLayout>
    );
};

export default ImportPage;
