// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';

import {
    normalizeMnemonics,
    validateMnemonics,
} from '_shared/cryptography/mnemonics';
import { useAppDispatch, useAppSelector } from '_src/ui/app/hooks';
import { createMnemonic, setMnemonic } from '_src/ui/app/redux/slices/account';

import type { ChangeEventHandler } from 'react';

import st from './Import.module.scss';

const ImportPage = () => {
    const [mnemonicInput, setMnemonicInput] = useState('');
    const [mnemonicValid, setMnemonicValid] = useState<boolean | null>(null);
    const onMnemonicChange = useCallback<
        ChangeEventHandler<HTMLTextAreaElement>
    >((e) => {
        setMnemonicInput(e.target.value);
        setMnemonicValid(null);
    }, []);
    const onHandleInputBlur = useCallback(() => {
        const adjMnemonic = normalizeMnemonics(mnemonicInput);
        setMnemonicInput(adjMnemonic);
        setMnemonicValid(validateMnemonics(adjMnemonic));
    }, [mnemonicInput]);
    const createInProgress = useAppSelector(({ account }) => account.creating);
    const dispatch = useAppDispatch();
    const onHandleImport = useCallback(async () => {
        await dispatch(createMnemonic(mnemonicInput));
        dispatch(setMnemonic(mnemonicInput));
    }, [mnemonicInput, dispatch]);
    return (
        <>
            <h1>Import existing wallet</h1>
            <textarea
                rows={5}
                onChange={onMnemonicChange}
                value={mnemonicInput}
                onBlur={onHandleInputBlur}
                className={st.mnemonic}
                placeholder="Insert your Recovery Passphrase"
                disabled={createInProgress}
            />
            <div className={st.error}>
                {mnemonicValid === false
                    ? 'Recovery Passphrase is not valid'
                    : ''}
            </div>
            <button
                type="button"
                className="btn"
                disabled={!mnemonicInput || createInProgress || !mnemonicValid}
                onClick={onHandleImport}
            >
                Import
            </button>
        </>
    );
};

export default ImportPage;
