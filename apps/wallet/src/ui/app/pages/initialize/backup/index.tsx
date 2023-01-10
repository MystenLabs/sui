// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import Button from '_app/shared/button';
import CardLayout from '_app/shared/card-layout';
import { Text } from '_app/shared/text';
import { useLockedGuard } from '_app/wallet/hooks';
import Alert from '_components/alert';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { useAppDispatch } from '_hooks';
import { loadEntropyFromKeyring } from '_redux/slices/account';
import { entropyToMnemonic, toEntropy } from '_shared/utils/bip39';

export type BackupPageProps = {
    mode?: 'created' | 'imported';
};

const BackupPage = ({ mode = 'created' }: BackupPageProps) => {
    const guardsLoading = useLockedGuard(false);
    const [loading, setLoading] = useState(true);
    const [mnemonic, setLocalMnemonic] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [passwordCopied, setPasswordCopied] = useState(false);
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
                title={
                    mode === 'imported'
                        ? 'Wallet Imported Successfully!'
                        : 'Wallet Created Successfully!'
                }
            >
                {mode === 'created' ? (
                    <>
                        <div className="mb-1 mt-7.5">
                            <Text
                                variant="caption"
                                color="steel-darker"
                                weight="bold"
                            >
                                Recovery phrase
                            </Text>
                        </div>
                        <div className="mb-3.5 mt-2 text-center">
                            <Text
                                variant="p2"
                                color="steel-dark"
                                weight="normal"
                            >
                                Your recovery phrase makes it easy to back up
                                and restore your account.
                            </Text>
                        </div>
                        <Loading loading={loading}>
                            {mnemonic ? (
                                <div className="text-steel-dark flex flex-col flex-nowrap gap-2 self-stretch font-semibold text-heading5 p-3.5 rounded-[15px] bg-white border border-solid border-[#EBECED] shadow-button leading-snug">
                                    {mnemonic}
                                </div>
                            ) : (
                                <Alert>{error}</Alert>
                            )}
                        </Loading>
                        <div className="mt-3.75 mb-1 text-center">
                            <Text
                                variant="caption"
                                color="steel-dark"
                                weight="semibold"
                            >
                                Warning
                            </Text>
                        </div>
                        <div className="mb-1 text-center">
                            <Text
                                variant="p2"
                                color="steel-dark"
                                weight="normal"
                            >
                                Never disclose your secret recovery phrase.
                                Anyone with the passphrase can take over your
                                account forever.
                            </Text>
                        </div>
                        <div className="flex-1" />
                        <div className="w-full text-left flex mt-5">
                            <label className="flex items-center justify-center h-5 mb-0 mr-5 text-sui-dark gap-1.25 relative cursor-pointer">
                                <input
                                    type="checkbox"
                                    name="agree"
                                    id="agree"
                                    className="peer/agree invisible ml-2"
                                    onChange={() =>
                                        setPasswordCopied(!passwordCopied)
                                    }
                                />
                                <div className="absolute top-0 left-0 h-5 w-5 bg-white peer-checked/agree:bg-success peer-checked/agree:shadow-none border-gray-50 border rounded shadow-button flex justify-center items-center">
                                    <Icon
                                        icon={SuiIcons.Checkmark}
                                        className="text-white text-[8px] font-semibold"
                                    />
                                </div>

                                <Text
                                    variant="bodySmall"
                                    color="steel-dark"
                                    weight="normal"
                                >
                                    I saved my recovery passphrase
                                </Text>
                            </label>
                        </div>
                    </>
                ) : null}
                <div className="flex-1" />
                <Button
                    type="button"
                    className="flex flex-nowrap self-stretch px-3.5 py-5"
                    size="large"
                    mode="primary"
                    disabled={mode === 'created' && !passwordCopied}
                    onClick={() => navigate('/')}
                >
                    Open Sui Wallet
                    <Icon
                        icon={SuiIcons.ArrowLeft}
                        className="text-p2 font-normal rotate-135"
                    />
                </Button>
            </CardLayout>
        </Loading>
    );
};

export default BackupPage;
