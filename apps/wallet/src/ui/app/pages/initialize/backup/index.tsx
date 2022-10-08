// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import Button from '_app/shared/button';
import Alert from '_components/alert';
import CopyToClipboard from '_components/copy-to-clipboard';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { useAppDispatch } from '_hooks';
import CardLayout from '_pages/initialize/shared/card-layout';
import { loadMnemonicFromKeyring } from '_redux/slices/account';

import st from './Backup.module.scss';

const BackupPage = () => {
    const [loading, setLoading] = useState(true);
    const [mnemonic, setLocalMnemonic] = useState<string | null>(null);
    const navigate = useNavigate();
    const dispatch = useAppDispatch();
    useEffect(() => {
        // TODO: this assumes that the Keyring in bg service is unlocked. It should be fix
        // when we add a locked status guard. (#encrypt-wallet)
        (async () => {
            setLoading(true);
            try {
                setLocalMnemonic(
                    await dispatch(loadMnemonicFromKeyring({})).unwrap()
                );
            } catch (e) {
                // Do nothing
            } finally {
                setLoading(false);
            }
        })();
    }, [dispatch]);
    return (
        <CardLayout>
            <div className={st.successIcon}>
                <div className={st.successBg}>
                    <Icon icon={SuiIcons.ThumbsUp} className={st.thumbsUp} />
                </div>
            </div>
            <h1 className={st.headerTitle}>Wallet Created Successfully!</h1>
            <h2 className={st.subTitle}>Recovery Phrase</h2>
            <Loading loading={loading}>
                {mnemonic ? (
                    <div className={st.mnemonic}>
                        {mnemonic}
                        <CopyToClipboard
                            txt={mnemonic}
                            className={st.copy}
                            mode="plain"
                        >
                            COPY
                        </CopyToClipboard>
                    </div>
                ) : (
                    <Alert>Something is wrong, Recovery Phrase is empty.</Alert>
                )}
            </Loading>
            <div className={st.info}>
                Your recovery phrase makes it easy to back up and restore your
                account.
            </div>
            <div className={st.info}>
                <div className={st.infoCaption}>WARNING</div>
                Never disclose your secret recovery phrase. Anyone with the
                passphrase can take over your account forever.
            </div>
            <Button
                type="button"
                className={st.btn}
                size="large"
                mode="primary"
                onClick={() => navigate('/')}
            >
                Open Sui Wallet
                <Icon icon={SuiIcons.ArrowLeft} className={st.arrowUp} />
            </Button>
        </CardLayout>
    );
};

export default BackupPage;
