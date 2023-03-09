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

export async function getSuiLedgerClient(): Promise<SuiLedgerClient> {
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
