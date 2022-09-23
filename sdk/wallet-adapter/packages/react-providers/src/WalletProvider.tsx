// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { FC, ReactNode, useCallback, useEffect, useState } from "react";
import type { SuiAddress, MoveCallTransaction, SuiTransactionResponse } from '@mysten/sui.js';
import { WalletCapabilities } from "@mysten/wallet-adapter-base";
import { Wallet, WalletContext } from './useWallet';

export interface WalletAdapter {
    adapter: WalletCapabilities;
}

export interface WalletProviderProps {
    children: ReactNode;
    // Pass this through props to add list of supported wallets
    supportedWallets: Wallet[]
}

export const WalletProvider: FC<WalletProviderProps> = ({
    children,
    supportedWallets
}) => {
    // Wallet that user chose
    const [wallet, setWallet] = useState<WalletAdapter | null>(null);
    const [connected, setConnected] = useState(false)
    const [connecting, setConnecting] = useState(false)


    const connect = useCallback(
        async () => {
            if (wallet == null) {
                return;
            }
            try {
                setConnecting(true);
                await wallet.adapter.connect()
                setConnected(true)
            } catch (e) {
                setConnected(false)
            }
            setConnecting(false);
    }, [wallet]);

    const disconnect = async () => {
        setConnected(false);
        setWalletAndUpdateStorage(null);
    }

    // Use this to update wallet so that the chosen wallet persists after reload.
    const setWalletAndUpdateStorage = useCallback((selectedWallet: Wallet | null) => {
        setWallet(selectedWallet);
        if (selectedWallet != null) {
            localStorage.setItem('suiWallet', selectedWallet.adapter.name);
        } else {
            localStorage.removeItem('suiWallet');
        }
    }, []);

    // Changes the selected wallet
    const choose = useCallback((name: string) => {
        let newWallet = supportedWallets.find(wallet => wallet.adapter.name === name);
        if (newWallet) {
            setWalletAndUpdateStorage(newWallet);
        }
        connect();
    }, [supportedWallets, connect, setWalletAndUpdateStorage]);

    // If the wallet is null, check if there isn't anything in local storage
    // Note: Optimize this.
    useEffect(() => {
        if (!wallet && !connected && !connecting) {
            let walletItem = localStorage.getItem('suiWallet');
            if (typeof walletItem === 'string') {
                const items = walletItem;
                choose(items);
            }
        }
    }, [choose, connected, connecting, wallet]);

    // Returns all accounts (i.e. public keys) managed by the selected wallet
    const getAccounts = async (): Promise<SuiAddress[]> => {
        if (wallet == null) throw Error('Wallet Not Connected');
        return await wallet.adapter.getAccounts();
    }

    // Requests wallet for signature and executes if signed
    const executeMoveCall = async (transaction: MoveCallTransaction): Promise<SuiTransactionResponse> => {
        if (wallet == null) throw Error('Wallet Not Connected');
        return await wallet.adapter.executeMoveCall(transaction);
    }

    // Requests wallet for signature on serialized transaction and executes if signed
    const executeSerializedMoveCall = async (transactionBytes: Uint8Array): Promise<SuiTransactionResponse> => {
        if (wallet == null) throw Error('Wallet Not Connected');
        return await wallet.adapter.executeSerializedMoveCall(transactionBytes);
    }

    // Attempt to connect whenever user selects a new wallet
    useEffect(() => {
        if (
            wallet != null &&
            connecting !== true &&
            connected !== true
        ) {
            connect();
        }
    }, [connect, wallet, connecting, connected])

    // Whenever the user selectes a new wallet
    return (
        <WalletContext.Provider value={{
                supportedWallets,
                wallet,
                connecting: connecting,
                connected: connected,
                select: choose,
                connect,
                disconnect,
                getAccounts,
                executeMoveCall,
                executeSerializedMoveCall
        }}>
            {children}
        </WalletContext.Provider>
    );
};
