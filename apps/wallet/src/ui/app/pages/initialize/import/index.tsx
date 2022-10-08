// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';

import StepOne from './steps/StepOne';
import { WALLET_ENCRYPTION_ENABLED } from '_app/wallet/constants';
import { useAppDispatch } from '_hooks';
import CardLayout from '_pages/initialize/shared/card-layout';
import { createMnemonic } from '_redux/slices/account';

import st from './Import.module.scss';

const initialValues = {
    mnemonic: '',
    password: '',
    confirmPassword: '',
};

const allSteps = [StepOne];

if (WALLET_ENCRYPTION_ENABLED) {
    // TODO: add more steps
}

export type ImportValuesType = typeof initialValues;

const ImportPage = () => {
    const [data, setData] = useState<ImportValuesType>(initialValues);
    const [step, setStep] = useState(0);
    const dispatch = useAppDispatch();
    const onHandleSubmit = useCallback(
        async ({ mnemonic, password }: ImportValuesType) => {
            //TODO
            await new Promise((r) => setTimeout(r, 3000));
            await dispatch(
                createMnemonic({ importedMnemonic: mnemonic, password })
            );
        },
        [dispatch]
    );
    const totalSteps = allSteps.length;
    const StepForm = step < totalSteps ? allSteps[step] : null;
    return (
        <CardLayout>
            <h3 className={st.headerSubTitle}>Wallet Setup</h3>
            <h1 className={st.headerTitle}>Import an Existing Wallet</h1>
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
