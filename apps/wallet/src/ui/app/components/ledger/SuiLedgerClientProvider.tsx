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
    connectToLedger: () => Promise<SuiLedgerClient>;
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

    const connectToLedger = useCallback(async () => {
        if (suiLedgerClient?.transport) {
            // If we've already connected to a Ledger device, we need
            // to close the connection before we try to re-connect
            await suiLedgerClient.transport.close();
        }

        const ledgerTransport = await requestConnectionToLedger();
        const ledgerClient = new SuiLedgerClient(ledgerTransport);
        setSuiLedgerClient(ledgerClient);
        return ledgerClient;
    }, [suiLedgerClient]);

    const contextValue: SuiLedgerClientContextValue = useMemo(() => {
        return {
            suiLedgerClient,
            connectToLedger,
        };
    }, [connectToLedger, suiLedgerClient]);

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
            'useSuiLedgerClient use must be within SuiLedgerClientContext'
        );
    }
    return suiLedgerClientContext;
}

async function requestConnectionToLedger() {
    try {
        if (await TransportWebHID.isSupported()) {
            return await TransportWebHID.request();
        } else if (await TransportWebUSB.isSupported()) {
            return await TransportWebUSB.request();
        }
    } catch (error) {
        throw new LedgerConnectionFailedError(
            "Unable to connect to the user's Ledger device"
        );
    }
    throw new LedgerNoTransportMechanismError(
        "There are no supported transport mechanisms to connect to the user's Ledger device"
    );
}
