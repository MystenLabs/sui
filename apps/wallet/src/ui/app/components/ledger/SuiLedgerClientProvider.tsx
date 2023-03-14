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

import type Transport from '@ledgerhq/hw-transport';
import { LedgerSigner } from '../../LedgerSigner';
import { api } from '../../redux/store/thunk-extras';
import { useAppSelector } from '../../hooks';

type SuiLedgerClientProviderProps = {
    children: React.ReactNode;
};

type SuiLedgerClientContextValue = [
    SuiLedgerClient | undefined,
    () => Promise<SuiLedgerClient>,
    (derivationPath: string) => Promise<LedgerSigner>
];

const SuiLedgerClientContext = createContext<
    SuiLedgerClientContextValue | undefined
>(undefined);

type LedgerSignerByDerivationPath = Map<string, LedgerSigner>;

export function SuiLedgerClientProvider({
    children,
}: SuiLedgerClientProviderProps) {
    const [suiLedgerClient, setSuiLedgerClient] = useState<SuiLedgerClient>();
    const network = useAppSelector(
        ({ app: { apiEnv, customRPC } }) => `${apiEnv}_${customRPC}`
    );
    const [ledgerSignerMap, setLedgerSignerMap] =
        useState<LedgerSignerByDerivationPath>(new Map());

    useEffect(() => {
        const onDisconnect = () => {
            setSuiLedgerClient(undefined);
            console.log('disconnected');
            setLedgerSignerMap(new Map());
        };

        suiLedgerClient?.transport.on('disconnect', onDisconnect);
        return () => suiLedgerClient?.transport.off('disconnect', onDisconnect);
    }, [suiLedgerClient?.transport]);

    const getLedgerSignerInstance = useCallback(
        async (derivationPath: string) => {
            console.log('GETTING INSTACE', derivationPath);
            const existingLedgerSigner = ledgerSignerMap.get(derivationPath);
            if (existingLedgerSigner) {
                console.log('INSTANCE FOUND');
                return existingLedgerSigner;
            }

            let ledgerSigner: LedgerSigner;

            if (!suiLedgerClient) {
                try {
                    const transport = await getLedgerTransport(false);
                    const newClient = new SuiLedgerClient(transport);
                    setSuiLedgerClient(newClient);

                    ledgerSigner = new LedgerSigner(
                        newClient,
                        derivationPath,
                        api.instance.fullNode
                    );
                } catch (e) {
                    throw new Error('F');
                }
            } else {
                ledgerSigner = new LedgerSigner(
                    suiLedgerClient,
                    derivationPath,
                    api.instance.fullNode
                );
            }

            setLedgerSignerMap((prevState) => {
                const updatedMap = new Map(prevState);
                updatedMap.set(derivationPath, ledgerSigner!);
                return updatedMap;
            });

            console.log('returning signer');
            return ledgerSigner;
        },
        [ledgerSignerMap, suiLedgerClient]
    );

    const connectToLedger = useCallback(async () => {
        console.log('ATTEMPTING TO CONNECT', suiLedgerClient);
        if (suiLedgerClient?.transport) {
            // If we've already connected to a Ledger device, we need
            // to close the connection before we try to re-connect
            console.log(
                'CLOSING ALREADY OPEN TRANSPORT',
                suiLedgerClient.transport
            );
            await suiLedgerClient.transport.close();
        }

        const ledgerTransport = await getLedgerTransport(true);
        const ledgerClient = new SuiLedgerClient(ledgerTransport);
        console.log('SETTING STATE', ledgerClient);
        setSuiLedgerClient(ledgerClient);
        return ledgerClient;
    }, [suiLedgerClient]);

    const contextValue: SuiLedgerClientContextValue = useMemo(
        () => [suiLedgerClient, connectToLedger, getLedgerSignerInstance],
        [connectToLedger, suiLedgerClient, getLedgerSignerInstance]
    );

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

async function getLedgerTransport(requestPermissionsFirst: boolean) {
    let ledgerTransport: Transport | null | undefined;

    try {
        if (requestPermissionsFirst) {
            ledgerTransport = await requestConnectToLedger();
        } else {
            ledgerTransport = await connectToLedger();
        }
    } catch (error) {
        console.log('ERROR', error);
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

async function requestConnectToLedger(): Promise<Transport | null> {
    if (await TransportWebHID.isSupported()) {
        return await TransportWebHID.request();
    } else if (await TransportWebUSB.isSupported()) {
        return await TransportWebUSB.request();
    }
    return null;
}

async function connectToLedger(): Promise<Transport | null> {
    if (await TransportWebHID.isSupported()) {
        return await TransportWebHID.openConnected();
    } else if (await TransportWebUSB.isSupported()) {
        return await TransportWebUSB.openConnected();
    }
    return null;
}
