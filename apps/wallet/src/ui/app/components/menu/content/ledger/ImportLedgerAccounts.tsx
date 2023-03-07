// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNavigate } from 'react-router-dom';

import { SummaryCard } from '../../../SummaryCard';
import Overlay from '../../../overlay';
import { useNextMenuUrl } from '../../hooks';
import { Button } from '_src/ui/app/shared/ButtonUI';
import { LockUnlocked16 as UnlockedLockIcon } from '@mysten/icons';
import { Link } from '_src/ui/app/shared/Link';
import { LedgerAccount } from './LedgerAccount';

const mockAccounts = [
    {
        isSelected: false,
        address:
            '0x7a286c8455a801f6d81faaa0f87543fa4a0de64dcc48b9c9308ee18f0f6ccdd3',
        balance: 30,
    },
    {
        isSelected: true,
        address:
            '0x7a286c8455a401f6d81faaa0f87543fa4a0de64dcc48b9c9308ee18f0f6ccdd3',
        balance: 30000,
    },
];

export function ImportLedgerAccounts() {
    const accountsUrl = useNextMenuUrl(true, `/accounts`);
    const navigate = useNavigate();

    return (
        <Overlay
            showModal
            title="Import Accounts"
            closeOverlay={() => {
                navigate(accountsUrl);
            }}
        >
            <div className="w-full flex flex-col">
                <SummaryCard
                    minimalPadding
                    header="Connect Ledger Accounts"
                    body={
                        <ul className="list-none h-[261px] m-0 p-0 -mr-2 mt-1 py-0 pr-2 overflow-auto custom-scrollbar">
                            {mockAccounts.map((account) => {
                                return (
                                    <li
                                        className="pt-2 pb-2 first:pt-1"
                                        key={account.address}
                                    >
                                        <LedgerAccount
                                            isSelected={account.isSelected}
                                            address={account.address}
                                            balance={account.balance}
                                        />
                                    </li>
                                );
                            })}
                        </ul>
                    }
                    footer={
                        <div className="rounded-b-2xl text-center">
                            <Link
                                text="Select All Accounts"
                                color="heroDark"
                                weight="medium"
                            />
                        </div>
                    }
                />
                <div className="flex gap-2.5 mt-5">
                    <Button
                        variant="secondary"
                        text="Cancel"
                        to={accountsUrl}
                    />
                    <Button
                        variant="primary"
                        before={<UnlockedLockIcon />}
                        text="Unlock"
                        onClick={() => {
                            // TODO: Do work to actually import the selected accounts once we have
                            // the account infrastructure setup to support Ledger accounts
                            navigate(accountsUrl);
                        }}
                    />
                </div>
            </div>
        </Overlay>
    );
}
