// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type JsonRpcProvider } from '@mysten/sui.js';
import { createContext, useContext, useState } from 'react';

import TransportWebHID from '@ledgerhq/hw-transport-webhid';
import TransportWebUSB from '@ledgerhq/hw-transport-webusb';
import SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';

import type Transport from '@ledgerhq/hw-transport';

let suiLedgerClient: SuiLedgerClient | undefined;

async function getLedgerTransport(): Promise<Transport | null> {
    if (await TransportWebHID.isSupported()) {
        return await TransportWebHID.request();
    } else if (await TransportWebUSB.isSupported()) {
        return await TransportWebUSB.request();
    }
    return null;
}

export function getSuiLedgerClient() {
    return suiLedgerClient;
}

export async function attemptConnectionAndGetSuiLedgerClient(): Promise<SuiLedgerClient> {
    if (!suiLedgerClient) {
        let ledgerTransport: Transport | null | undefined;

        try {
            ledgerTransport = await getLedgerTransport();
        } catch (error) {
            throw new Error('Ledger connection failed.');
        }

        if (!ledgerTransport) {
            throw new Error(
                "Your machine doesn't support HID or USB transport mechanisms."
            );
        }

        suiLedgerClient = new SuiLedgerClient(ledgerTransport);
    }
    return suiLedgerClient;
}

export const SuiLedgerClientContext = createContext<
    [SuiLedgerClient | undefined, () => void] | undefined
>(undefined);

type SuiLedgerClientProviderProps = {
    children: React.ReactNode;
};

export function SuiLedgerClientProvider({
    children,
}: SuiLedgerClientProviderProps) {
    const [suiLedgerClient, setSuiLedgerClient] = useState<SuiLedgerClient>();

    const connectToLedger = async () => {
        let ledgerTransport: Transport | null | undefined;

        try {
            ledgerTransport = await getLedgerTransport();
        } catch (error) {
            throw new Error('Ledger connection failed.');
        }

        if (!ledgerTransport) {
            throw new Error(
                "Your machine doesn't support HID or USB transport mechanisms."
            );
        }

        setSuiLedgerClient(new SuiLedgerClient(ledgerTransport));
    };

    return (
        <SuiLedgerClientContext.Provider
            value={[suiLedgerClient, connectToLedger]}
        >
            {children}
        </SuiLedgerClientContext.Provider>
    );
}

export function useSuiLedgerClient() {
    const suiLedgerClientContext = useContext(SuiLedgerClientContext);
    if (!suiLedgerClientContext) {
        throw new Error(
            'useSuiLedgerClient use must be within SuiLedgerClientContext'
        );
    }
    return suiLedgerClientContext;
}
