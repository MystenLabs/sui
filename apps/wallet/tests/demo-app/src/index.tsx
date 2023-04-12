// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionBlock } from '@mysten/sui.js';
import {
    type ReadonlyWalletAccount,
    WalletAccount,
    getWallets,
} from '@mysten/wallet-standard';
import { useEffect, useState } from 'react';
import ReactDOM from 'react-dom/client';

import { type SuiWallet } from '_src/dapp-interface/WalletStandardInterface';

function App() {
    const [suiWallet, setSuiWallet] = useState<SuiWallet | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [accounts, setAccounts] = useState<ReadonlyWalletAccount[]>([]);

    useEffect(() => {
        const walletsApi = getWallets();
        function updateWallets() {
            const updatedWallets = walletsApi.get();
            setSuiWallet(
                (updatedWallets.find((aWallet) =>
                    aWallet.name.includes('Sui Wallet')
                ) || null) as SuiWallet | null
            );
        }
        updateWallets();
        const unregister1 = walletsApi.on('register', updateWallets);
        const unregister2 = walletsApi.on('unregister', updateWallets);
        return () => {
            unregister1();
            unregister2();
        };
    }, []);
    useEffect(() => {
        if (suiWallet) {
            setAccounts(suiWallet.accounts);
            return suiWallet.features['standard:events'].on(
                'change',
                ({ accounts }) => {
                    if (accounts) {
                        setAccounts(suiWallet.accounts);
                    }
                }
            );
        }
    }, [suiWallet]);
    if (!suiWallet) {
        return <h1>Sui Wallet not found</h1>;
    }
    return (
        <>
            <h1>Sui Wallet is installed. ({suiWallet.name})</h1>
            {accounts.length ? (
                <ul>
                    {accounts.map((anAccount) => (
                        <li key={anAccount.address}>{anAccount.address}</li>
                    ))}
                </ul>
            ) : (
                <button
                    onClick={async () =>
                        suiWallet.features['standard:connect'].connect()
                    }
                >
                    Connect
                </button>
            )}
            <button
                onClick={async () => {
                    setError(null);
                    const txb = new TransactionBlock();
                    try {
                        await suiWallet.features[
                            'sui:signAndExecuteTransactionBlock'
                        ].signAndExecuteTransactionBlock({
                            transactionBlock: txb,
                            account: {} as ReadonlyWalletAccount,
                            chain: 'sui:unknown',
                        });
                    } catch (e) {
                        setError((e as Error).message);
                    }
                }}
            >
                Sent transaction no account
            </button>
            <button
                onClick={async () => {
                    setError(null);
                    const txb = new TransactionBlock();
                    try {
                        await suiWallet.features[
                            'sui:signAndExecuteTransactionBlock'
                        ].signAndExecuteTransactionBlock({
                            transactionBlock: txb,
                            account: {
                                address: '12345',
                            } as ReadonlyWalletAccount,
                            chain: 'sui:unknown',
                        });
                    } catch (e) {
                        setError((e as Error).message);
                    }
                }}
            >
                Sent transaction
            </button>
            <button
                onClick={async () => {
                    setError(null);
                    const txb = new TransactionBlock();
                    try {
                        await suiWallet.features[
                            'sui:signTransactionBlock'
                        ].signTransactionBlock({
                            transactionBlock: txb,
                            account: {} as ReadonlyWalletAccount,
                            chain: 'sui:unknown',
                        });
                    } catch (e) {
                        setError((e as Error).message);
                    }
                }}
            >
                Sign transaction no account
            </button>
            <button
                onClick={async () => {
                    setError(null);
                    const txb = new TransactionBlock();
                    try {
                        await suiWallet.features[
                            'sui:signTransactionBlock'
                        ].signTransactionBlock({
                            transactionBlock: txb,
                            account: {
                                address: '12345',
                            } as ReadonlyWalletAccount,
                            chain: 'sui:unknown',
                        });
                    } catch (e) {
                        setError((e as Error).message);
                    }
                }}
            >
                Sign transaction
            </button>
            <button
                onClick={async () => {
                    setError(null);
                    try {
                        await suiWallet.features['sui:signMessage'].signMessage(
                            {
                                account: {} as ReadonlyWalletAccount,
                                message: new TextEncoder().encode(
                                    'Test message'
                                ),
                            }
                        );
                    } catch (e) {
                        setError((e as Error).message);
                    }
                }}
            >
                Sign message no account
            </button>
            <button
                onClick={async () => {
                    setError(null);
                    try {
                        await suiWallet.features['sui:signMessage'].signMessage(
                            {
                                account: {
                                    address: '12345',
                                } as ReadonlyWalletAccount,
                                message: new TextEncoder().encode(
                                    'Test message'
                                ),
                            }
                        );
                    } catch (e) {
                        setError((e as Error).message);
                    }
                }}
            >
                Sign message
            </button>
            {error ? (
                <div>
                    <h6>Error</h6>
                    <div>{error}</div>
                </div>
            ) : null}
        </>
    );
}

ReactDOM.createRoot(document.getElementById('root')!).render(<App />);
