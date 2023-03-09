// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import TransportWebHID from '@ledgerhq/hw-transport-webhid';
import TransportWebUSB from '@ledgerhq/hw-transport-webusb';
import SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';
import { createContext, useContext, useState } from 'react';

import type Transport from '@ledgerhq/hw-transport';

export const SuiLedgerClientContext = createContext<
    [SuiLedgerClient | undefined, () => Promise<SuiLedgerClient>] | undefined
>(undefined);

type SuiLedgerClientProviderProps = {
    children: React.ReactNode;
};

export function SuiLedgerClientProvider({
    children,
}: SuiLedgerClientProviderProps) {
    const [suiLedgerClient, setSuiLedgerClient] = useState<SuiLedgerClient>();

    const connectToLedger = async () => {
        const ledgerTransport = await getLedgerTransport();
        const ledgerClient = new SuiLedgerClient(ledgerTransport);
        setSuiLedgerClient(ledgerClient);
        return ledgerClient;
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

async function getLedgerTransport() {
    let ledgerTransport: Transport | null | undefined;

    try {
        ledgerTransport = await initiateLedgerConnection();
    } catch (error) {
        throw new Error('Ledger connection failed.');
    }

    if (!ledgerTransport) {
        throw new Error(
            "Your machine doesn't support HID or USB transport mechanisms."
        );
    }

    return ledgerTransport;
}

async function initiateLedgerConnection(): Promise<Transport | null> {
    if (await TransportWebHID.isSupported()) {
        return await TransportWebHID.request();
    } else if (await TransportWebUSB.isSupported()) {
        return await TransportWebUSB.request();
    }
    return null;
}
