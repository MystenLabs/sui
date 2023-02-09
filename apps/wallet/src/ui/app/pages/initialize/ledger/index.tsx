// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import StepOne from './steps/StepOne';
import CardLayout from '_app/shared/card-layout';
import Alert from '_components/alert';
import { useAppDispatch } from '_hooks';
import { setLedgerAccount, logout } from '_redux/slices/account';
import { MAIN_UI_URL } from '_shared/utils';
import { thunkExtras } from '_store/thunk-extras';

const initialValues = {
    derivationPath: "m/44'/784'/0'/0/0",
    //password: '',
    //confirmPassword: '',
};

const allSteps = [StepOne];

export type LedgerValuesType = typeof initialValues;
export type ImportPageProps = {
    mode?: 'import' | 'forgot';
};
const ImportPage = ({ mode = 'import' }: ImportPageProps) => {
    const [error, setError] = useState<Error | null>(null);

    const [data, setData] = useState<LedgerValuesType>(initialValues);
    const [step, setStep] = useState(0);
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const onHandleSubmit = useCallback(
        async ({ derivationPath }: LedgerValuesType) => {
            const { api, initAppSui } = thunkExtras;
            try {
                const signer = api.getLedgerSignerInstance(
                    derivationPath,
                    initAppSui
                );
                const address = await signer.getAddress();
                await dispatch(
                    setLedgerAccount({
                        address,
                        derivationPath,
                    })
                );
                navigate('../backup-imported');
            } catch (e) {
                setError(e as Error);
            }
        },
        [dispatch, navigate, mode]
    );
    if (error && step !== 0) {
        setStep(0);
    }
    const totalSteps = allSteps.length;
    const StepForm = step < totalSteps ? allSteps[step] : null;
    return (
        <CardLayout
            headerCaption={mode === 'import' ? 'Wallet Setup' : undefined}
            title={'Connect to your Ledger Device'}
            mode={'box'}
        >
            {StepForm ? (
                <div className="mt-7.5 flex flex-col flex-nowrap items-stretch flex-1 flex-grow w-full">
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
                </div>
            ) : null}
            {error ? (
                <div className="mt-3">
                    <Alert>{error?.message}</Alert>
                </div>
            ) : null}
        </CardLayout>
    );
};

export default ImportPage;
