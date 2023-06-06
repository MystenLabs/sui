// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createKioskAndShare } from '@mysten/kiosk';
import { TransactionBlock } from '@mysten/sui.js';
import { useTransactionExecution } from '../../hooks/useTransactionExecution';
import { useWalletKit } from '@mysten/wallet-kit';
import { useState } from 'react';
import { Loading } from '../Base/Loading';

export function KioskCreation({ onCreate }: { onCreate: () => void }) {
  const { signAndExecute } = useTransactionExecution();
  const { currentAccount } = useWalletKit();
  const [loading, setLoading] = useState<boolean>(false);

  const createNewKiosk = async () => {
    if (!currentAccount?.address) return;
    setLoading(true);

    const tx = new TransactionBlock();
    const kiosk_cap = createKioskAndShare(tx);

    tx.transferObjects(
      [kiosk_cap],
      tx.pure(currentAccount?.address, 'address'),
    );

    await signAndExecute({ tx });
    onCreate();
    setLoading(false);
  };

  if (loading) return <Loading />;

  return (
    <div className="min-h-[70vh] flex items-center justify-center gap-4 mt-6 text-center">
      <div>
        <h2 className="font-bold text-2xl">You don't have a kiosk yet.</h2>
        <p>Create your kiosk to start trading.</p>
        <button onClick={createNewKiosk} className="mt-8">
          Create your Kiosk
        </button>
      </div>
    </div>
  );
}
