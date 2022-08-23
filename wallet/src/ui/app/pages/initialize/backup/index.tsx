// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';
import { useNavigate } from 'react-router-dom';

import Button from '_app/shared/button';
import Icon, { SuiIcons } from '_components/icon';
import { useAppDispatch, useAppSelector } from '_src/ui/app/hooks';
import { setMnemonic } from '_src/ui/app/redux/slices/account';

import st from './Backup.module.scss';

const BackupPage = () => {
    const mnemonic = useAppSelector(
        ({ account }) => account.createdMnemonic || account.mnemonic
    );
    const navigate = useNavigate();
    const dispatch = useAppDispatch();
    const handleOnClick = useCallback(() => {
        if (mnemonic) {
            navigate('/');
            dispatch(setMnemonic(mnemonic));
        }
    }, [navigate, dispatch, mnemonic]);
    return (
        <>
            <h1 className={st.headerTitle}>Wallet Successfully Created!</h1>
            <h2>Backup Recovery Passphrase</h2>
            <div className={st.mnemonic}>{mnemonic}</div>

            <Button
                type="button"
                onClick={handleOnClick}
                className="btn"
                size="large"
            >
                <Icon icon={SuiIcons.Checkmark} className={st.success} />
                Done
            </Button>
        </>
    );
};

export default BackupPage;
