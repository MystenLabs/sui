// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { ToS_LINK } from '_shared/constants';
import Loading from '_src/ui/app/components/loading';
import { useAppDispatch, useAppSelector } from '_src/ui/app/hooks';
import { createMnemonic } from '_src/ui/app/redux/slices/account';

import type { ChangeEventHandler } from 'react';

import st from './Create.module.scss';

const CreatePage = () => {
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const onHandleCreate = useCallback(async () => {
        await dispatch(createMnemonic());
        navigate('../backup');
    }, [dispatch, navigate]);
    const [termsAccepted, setTerms] = useState(false);
    const onHandleTerms = useCallback<ChangeEventHandler<HTMLInputElement>>(
        (event) => {
            const checked = event.target.checked;
            setTerms(checked);
        },
        []
    );
    const creatingMnemonic = useAppSelector(({ account }) => account.creating);
    return (
        <>
            <h1>Create new wallet</h1>
            <div className={st.desc}>
                Creating a wallet generates a Recovery Passphrase. Using it you
                can restore the wallet.
            </div>
            <label className={st.terms}>
                <input
                    type="checkbox"
                    onChange={onHandleTerms}
                    checked={termsAccepted}
                />
                <span>
                    I have read and agree to the{' '}
                    <a href={ToS_LINK} target="_blank" rel="noreferrer">
                        Terms of Service
                    </a>
                </span>
            </label>
            <div>
                <Loading loading={creatingMnemonic}>
                    <button
                        type="button"
                        onClick={onHandleCreate}
                        disabled={!termsAccepted || creatingMnemonic}
                        className="btn"
                    >
                        Create
                    </button>
                </Loading>
            </div>
        </>
    );
};

export default CreatePage;
