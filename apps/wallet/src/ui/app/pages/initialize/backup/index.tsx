// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import Button from '_app/shared/button';
import CardLayout from '_app/shared/card-layout';
import { useLockedGuard } from '_app/wallet/hooks';
import Alert from '_components/alert';
import CopyToClipboard from '_components/copy-to-clipboard';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { useAppDispatch } from '_hooks';
import { loadEntropyFromKeyring } from '_redux/slices/account';
import { entropyToMnemonic, toEntropy } from '_shared/utils/bip39';

import st from './Backup.module.scss';

export type BackupPageProps = {
    mode?: 'created' | 'imported';
};

const BackupPage = ({ mode = 'created' }: BackupPageProps) => {
    const guardsLoading = useLockedGuard(false);
    const [loading, setLoading] = useState(true);
    const [mnemonic, setLocalMnemonic] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const navigate = useNavigate();
    const dispatch = useAppDispatch();
    useEffect(() => {
        (async () => {
            if (guardsLoading || mode !== 'created') {
                return;
            }
            setLoading(true);
            try {
                setLocalMnemonic(
                    entropyToMnemonic(
                        toEntropy(
                            await dispatch(loadEntropyFromKeyring({})).unwrap()
                        )
                    )
                );
            } catch (e) {
                setError(
                    (e as Error).message ||
                        'Something is wrong, Recovery Phrase is empty.'
                );
            } finally {
                setLoading(false);
            }
        })();
    }, [dispatch, mode, guardsLoading]);
    return (
        <Loading loading={guardsLoading}>
            <CardLayout
                icon="success"
                title={`Wallet ${
                    mode === 'imported' ? 'Imported' : 'Created'
                } Successfully!`}
                subtitle={mode === 'created' ? 'Recovery Phrase' : undefined}
            >
                {mode === 'created' ? (
                    <>
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
                                <Alert>{error}</Alert>
                            )}
                        </Loading>
                        <div className={st.info}>
                            Your recovery phrase makes it easy to back up and
                            restore your account.
                        </div>
                        <div className={st.info}>
                            <div className={st.infoCaption}>WARNING</div>
                            Never disclose your secret recovery phrase. Anyone
                            with the passphrase can take over your account
                            forever.
                        </div>
                    </>
                ) : null}
                <div className={st.fill} />
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
        </Loading>
    );
};

export default BackupPage;
