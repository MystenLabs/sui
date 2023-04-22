// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowLeft16, Check12 } from '@mysten/icons';
import { useEffect, useState } from 'react';

import { Button } from '_app/shared/ButtonUI';
import { CardLayout } from '_app/shared/card-layout';
import { Text } from '_app/shared/text';
import { useLockedGuard } from '_app/wallet/hooks';
import Alert from '_components/alert';
import Loading from '_components/loading';
import { useAppDispatch } from '_hooks';
import { loadEntropyFromKeyring } from '_redux/slices/account';
import { entropyToMnemonic, toEntropy } from '_shared/utils/bip39';
import { HideShowDisplayBox } from '_src/ui/app/components/HideShowDisplayBox';

export type BackupPageProps = {
    mode?: 'created' | 'imported';
};

const BackupPage = ({ mode = 'created' }: BackupPageProps) => {
    const guardsLoading = useLockedGuard(false);
    const [loading, setLoading] = useState(true);
    const [mnemonic, setLocalMnemonic] = useState<string[] | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [passwordCopied, setPasswordCopied] = useState(false);
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
                    ).split(' ')
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
                <div className="flex flex-col flex-nowrap flex-grow h-full w-full">
                    {mode === 'created' ? (
                        <div className="flex flex-col flex-nowrap flex-grow mb-5">
                            <div className="mb-1 mt-7.5 text-center">
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
                                    variant="pBodySmall"
                                    color="steel-dark"
                                    weight="normal"
                                >
                                    Your recovery phrase makes it easy to back
                                    up and restore your account.
                                </Text>
                            </div>
                            <Loading loading={loading}>
                                {mnemonic ? (
                                    <HideShowDisplayBox
                                        value={mnemonic}
                                        hideCopy
                                    />
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
                                    variant="pBodySmall"
                                    color="steel-dark"
                                    weight="normal"
                                >
                                    Never disclose your secret recovery phrase.
                                    Anyone can take over your account with it.
                                </Text>
                            </div>
                            <div className="flex-1" />
                            <div className="w-full text-left flex mt-5 mb-">
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
                                    <span className="absolute top-0 left-0 h-5 w-5 bg-white peer-checked/agree:bg-success peer-checked/agree:shadow-none border-gray-50 border rounded shadow-button flex justify-center items-center">
                                        <Check12 className="text-white text-body font-semibold" />
                                    </span>

                                    <Text
                                        variant="bodySmall"
                                        color="steel-dark"
                                        weight="normal"
                                    >
                                        I saved my recovery phrase
                                    </Text>
                                </label>
                            </div>
                        </div>
                    ) : null}
                    {mode !== 'created' && <div className="flex-1 flex" />}
                    <Button
                        type="button"
                        size="tall"
                        variant="primary"
                        disabled={mode === 'created' && !passwordCopied}
                        to="/"
                        text="Open Sui Wallet"
                        after={
                            <ArrowLeft16 className="text-pBodySmall font-normal rotate-135" />
                        }
                    />
                </div>
            </CardLayout>
        </Loading>
    );
};

export default BackupPage;
