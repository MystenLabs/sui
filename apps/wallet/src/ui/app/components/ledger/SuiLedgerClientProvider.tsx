// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import TransportWebHID from '@ledgerhq/hw-transport-webhid';
import TransportWebUSB from '@ledgerhq/hw-transport-webusb';
import SuiLedgerClient from '@mysten/ledgerjs-hw-app-sui';
import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';

import {
	convertErrorToLedgerConnectionFailedError,
	LedgerDeviceNotFoundError,
	LedgerNoTransportMechanismError,
} from './ledgerErrors';

type SuiLedgerClientProviderProps = {
	children: React.ReactNode;
};

type SuiLedgerClientContextValue = {
	suiLedgerClient: SuiLedgerClient | undefined;
	connectToLedger: (requestPermissionsFirst?: boolean) => Promise<SuiLedgerClient>;
};

const SuiLedgerClientContext = createContext<SuiLedgerClientContextValue | undefined>(undefined);

export function SuiLedgerClientProvider({ children }: SuiLedgerClientProviderProps) {
	const [suiLedgerClient, setSuiLedgerClient] = useState<SuiLedgerClient>();
	const resetSuiLedgerClient = useCallback(async () => {
		await suiLedgerClient?.transport.close();
		setSuiLedgerClient(undefined);
	}, [suiLedgerClient]);

	useEffect(() => {
		// NOTE: The disconnect event is fired when someone physically disconnects
		// their Ledger device in addition to when user's exit out of an application
		suiLedgerClient?.transport.on('disconnect', resetSuiLedgerClient);
		return () => {
			suiLedgerClient?.transport.off('disconnect', resetSuiLedgerClient);
		};
	}, [resetSuiLedgerClient, suiLedgerClient?.transport]);

	const connectToLedger = useCallback(
		async (requestPermissionsFirst = false) => {
			// If we've already connected to a Ledger device, we need
			// to close the connection before we try to re-connect
			await resetSuiLedgerClient();

			const ledgerTransport = requestPermissionsFirst
				? await requestLedgerConnection()
				: await openLedgerConnection();
			const ledgerClient = new SuiLedgerClient(ledgerTransport);
			setSuiLedgerClient(ledgerClient);
			return ledgerClient;
		},
		[resetSuiLedgerClient],
	);
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
		throw new Error('useSuiLedgerClient must be used within SuiLedgerClientContext');
	}
	return suiLedgerClientContext;
}

async function requestLedgerConnection() {
	const ledgerTransportClass = await getLedgerTransportClass();
	try {
		return await ledgerTransportClass.request();
	} catch (error) {
		throw convertErrorToLedgerConnectionFailedError(error);
	}
}

async function openLedgerConnection() {
	const ledgerTransportClass = await getLedgerTransportClass();
	let ledgerTransport: TransportWebHID | TransportWebUSB | null | undefined;

	try {
		ledgerTransport = await ledgerTransportClass.openConnected();
	} catch (error) {
		throw convertErrorToLedgerConnectionFailedError(error);
	}
	if (!ledgerTransport) {
		throw new LedgerDeviceNotFoundError(
			"The user doesn't have a Ledger device connected to their machine",
		);
	}
	return ledgerTransport;
}

async function getLedgerTransportClass() {
	if (await TransportWebHID.isSupported()) {
		return TransportWebHID;
	} else if (await TransportWebUSB.isSupported()) {
		return TransportWebUSB;
	}
	throw new LedgerNoTransportMechanismError(
		"There are no supported transport mechanisms to connect to the user's Ledger device",
	);
}
