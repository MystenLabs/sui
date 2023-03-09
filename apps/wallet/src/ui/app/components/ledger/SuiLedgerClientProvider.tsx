// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import TransportWebHID from '@ledgerhq/hw-transport-webhid';
import TransportWebUSB from '@ledgerhq/hw-transport-webusb';
import SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';
import { createContext, useContext, useState } from 'react';

import {
    LedgerConnectionFailedError,
    LedgerNoTransportMechanismError,
} from './LedgerExceptions';

import type Transport from '@ledgerhq/hw-transport';

type SuiLedgerClientProviderProps = {
    children: React.ReactNode;
};

const SuiLedgerClientContext = createContext<
    [SuiLedgerClient | undefined, () => Promise<SuiLedgerClient>] | undefined
>(undefined);

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
        throw new LedgerConnectionFailedError(
            "Unable to connect to the user's Ledger device"
        );
    }

    if (!ledgerTransport) {
        throw new LedgerNoTransportMechanismError(
            "There are no supported transport mechanisms to connect to the user's Ledger device"
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
