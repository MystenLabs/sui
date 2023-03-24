// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import TransportWebHID from '@ledgerhq/hw-transport-webhid';
import TransportWebUSB from '@ledgerhq/hw-transport-webusb';
import SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';
import {
    createContext,
    useCallback,
    useContext,
    useEffect,
    useMemo,
    useState,
} from 'react';

import {
    LedgerConnectionFailedError,
    LedgerNoTransportMechanismError,
} from './LedgerExceptions';

type SuiLedgerClientProviderProps = {
    children: React.ReactNode;
};

type SuiLedgerClientContextValue = {
    suiLedgerClient: SuiLedgerClient | undefined;
    requestLedgerConnection: () => Promise<SuiLedgerClient>;
    forceLedgerConnection: () => Promise<SuiLedgerClient | null>;
};

const SuiLedgerClientContext = createContext<
    SuiLedgerClientContextValue | undefined
>(undefined);

export function SuiLedgerClientProvider({
    children,
}: SuiLedgerClientProviderProps) {
    const [suiLedgerClient, setSuiLedgerClient] = useState<SuiLedgerClient>();

    useEffect(() => {
        const onDisconnect = () => {
            setSuiLedgerClient(undefined);
        };

        suiLedgerClient?.transport.on('disconnect', onDisconnect);
        return () => suiLedgerClient?.transport.off('disconnect', onDisconnect);
    }, [suiLedgerClient?.transport]);

    const requestLedgerConnection = useCallback(async () => {
        if (suiLedgerClient?.transport) {
            // If we've already connected to a Ledger device, we need
            // to close the connection before we try to re-connect
            await suiLedgerClient.transport.close();
        }

        const ledgerTransport = await initiateLedgerConnectionRequest();
        const ledgerClient = new SuiLedgerClient(ledgerTransport);
        setSuiLedgerClient(ledgerClient);
        return ledgerClient;
    }, [suiLedgerClient]);

    const forceLedgerConnection = useCallback(async () => {
        if (suiLedgerClient?.transport) {
            // If we've already connected to a Ledger device, we need
            // to close the connection before we try to re-connect
            await suiLedgerClient.transport.close();
        }

        const ledgerTransport = await initiateForcefulLedgerConnection();
        console.log('trn', ledgerTransport);
        if (ledgerTransport) {
            const ledgerClient = new SuiLedgerClient(ledgerTransport);
            setSuiLedgerClient(ledgerClient);
            return ledgerClient;
        }
        return null;
    }, [suiLedgerClient]);

    const contextValue: SuiLedgerClientContextValue = useMemo(() => {
        return {
            suiLedgerClient,
            requestLedgerConnection,
            forceLedgerConnection,
        };
    }, [requestLedgerConnection, forceLedgerConnection, suiLedgerClient]);

    return (
        <SuiLedgerClientContext.Provider value={contextValue}>
            {children}
        </SuiLedgerClientContext.Provider>
    );
}

export function useSuiLedgerClient() {
    const suiLedgerClientContext = useContext(SuiLedgerClientContext);
    if (!suiLedgerClientContext) {
        throw new Error(
            'useSuiLedgerClient must be used within SuiLedgerClientContext'
        );
    }
    return suiLedgerClientContext;
}

async function initiateLedgerConnectionRequest() {
    const ledgerTransport = await getLedgerTransport();

    try {
        return await ledgerTransport.request();
    } catch (error) {
        // Ledger doesn't return well-structured errors, so we'll raise a new error here
        const errorMessage =
            error instanceof Error ? error.message : String(error);
        throw new LedgerConnectionFailedError(
            `Unable to connect to the user's Ledger device: ${errorMessage}`
        );
    }
}

async function initiateForcefulLedgerConnection() {
    const ledgerTransport = await getLedgerTransport();

    try {
        return await ledgerTransport.openConnected();
    } catch (error) {
        // Ledger doesn't return well-structured errors, so we'll raise a new error here
        const errorMessage =
            error instanceof Error ? error.message : String(error);
        throw new LedgerConnectionFailedError(
            `Unable to connect to the user's Ledger device: ${errorMessage}`
        );
    }
}

async function getLedgerTransport() {
    if (await TransportWebHID.isSupported()) {
        return await TransportWebHID;
    } else if (await TransportWebUSB.isSupported()) {
        return await TransportWebUSB;
    }
    throw new LedgerNoTransportMechanismError(
        "There are no supported transport mechanisms to connect to the user's Ledger device"
    );
}
