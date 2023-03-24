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
    convertErrorToLedgerConnectionFailedError,
    LedgerDeviceNotFoundError,
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

    const resetSuiLedgerClient = useCallback(async () => {
        // If we've already connected to a Ledger device, we need
        // to close the connection before we try to re-connect
        if (suiLedgerClient) {
            await suiLedgerClient?.transport.close();
            setSuiLedgerClient(undefined);
        }
    }, [suiLedgerClient]);

    const requestLedgerConnection = useCallback(async () => {
        await resetSuiLedgerClient();
        const ledgerTransport = await initiateLedgerConnectionRequest();
        const ledgerClient = new SuiLedgerClient(ledgerTransport);
        setSuiLedgerClient(ledgerClient);
        return ledgerClient;
    }, [resetSuiLedgerClient]);

    const forceLedgerConnection = useCallback(async () => {
        resetSuiLedgerClient();
        const ledgerTransport = await initiateLedgerConnection();
        const ledgerClient = new SuiLedgerClient(ledgerTransport!);
        setSuiLedgerClient(ledgerClient);
        return ledgerClient;
    }, [resetSuiLedgerClient]);

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
    const ledgerTransportClass = await getLedgerTransportClass();
    try {
        return await ledgerTransportClass.request();
    } catch (error) {
        throw convertErrorToLedgerConnectionFailedError(error);
    }
}

async function initiateLedgerConnection() {
    const ledgerTransportClass = await getLedgerTransportClass();
    let ledgerTransport: TransportWebHID | TransportWebUSB | null | undefined;

    try {
        ledgerTransport = await ledgerTransportClass.openConnected();
    } catch (error) {
        throw convertErrorToLedgerConnectionFailedError(error);
    }
    if (!ledgerTransport) {
        throw new LedgerDeviceNotFoundError(
            "The user doesn't have a Ledger device connected to their machine"
        );
    }
}

async function getLedgerTransportClass() {
    if (await TransportWebHID.isSupported()) {
        return await TransportWebHID;
    } else if (await TransportWebUSB.isSupported()) {
        return await TransportWebUSB;
    }
    throw new LedgerNoTransportMechanismError(
        "There are no supported transport mechanisms to connect to the user's Ledger device"
    );
}
