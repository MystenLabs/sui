// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from '@mysten/wallet-kit';
import { Tab } from '@headlessui/react';
import { OwnedObjects } from '../Inventory/OwnedObjects';
import { KioskItems } from './KioskItems';
import { Kiosk, getKioskObject, withdrawFromKiosk } from '@mysten/kiosk';
import { useEffect, useState } from 'react';
import { TransactionBlock, formatAddress } from '@mysten/sui.js';
import { ExplorerLink } from '../Base/ExplorerLink';
import { formatSui, mistToSui } from '../../utils/utils';
import { useTransactionExecution } from '../../hooks/useTransactionExecution';
import { toast } from 'react-hot-toast';
import { useRpc } from '../../context/RpcClientContext';
import { useOwnedKiosk } from '../../hooks/kiosk';

export function KioskData() {
  const provider = useRpc();
  const { currentAccount } = useWalletKit();
  const [kiosk, setKiosk] = useState<Kiosk | undefined>(undefined);

  const { data: ownedKiosk } = useOwnedKiosk();

  const kioskId = ownedKiosk?.kioskId;
  const kioskOwnerCap = ownedKiosk?.kioskCap;

  const { signAndExecute } = useTransactionExecution();

  useEffect(() => {
    if (!kioskId) return;
    if (!kiosk && ownedKiosk?.kioskId) {
      getKioskObject(provider, kioskId).then((res) => setKiosk(res));
    }
  }, [kioskId]);

  const withdrawProfits = async () => {
    if (!kiosk || !kioskId || !kioskOwnerCap || !currentAccount?.address)
      return;

    const tx = new TransactionBlock();
    const coin = withdrawFromKiosk(tx, kioskId, kioskOwnerCap, kiosk.profits);

    tx.transferObjects([coin], tx.pure(currentAccount.address, 'address'));

    const success = await signAndExecute({ tx });

    if (success) toast.success('Profits withdrawn successfully');
  };

  const profits = formatSui(mistToSui(kiosk?.profits));

  return (
    <div className="container">
      <div className="my-12 ">
        {kiosk && (
          <div className="gap-5 items-center">
            <div>
              Selected Kiosk:{' '}
              {
                <ExplorerLink
                  text={formatAddress(kiosk.id)}
                  object={kiosk.id}
                />
              }
            </div>
            <div className="mt-2">
              Owner (displayed): (
              <ExplorerLink
                text={formatAddress(kiosk.owner)}
                address={kiosk.owner}
              />
              )
            </div>
            <div className="mt-2">Items Count: {kiosk.itemCount}</div>
            <div className="mt-2">
              Profits: {profits} SUI
              {Number(kiosk.profits) > 0 && (
                <button
                  className="text-xs !py-1 ml-3"
                  onClick={withdrawProfits}
                >
                  Withdraw all
                </button>
              )}
            </div>
            <div className="mt-2">
              UID Exposed: {kiosk.allowExtensions.toString()}{' '}
            </div>
          </div>
        )}
      </div>

      <Tab.Group vertical defaultIndex={0}>
        <Tab.List>
          <Tab className="tab-title">My Kiosk</Tab>
          <Tab className="tab-title">My Wallet</Tab>
        </Tab.List>
        <Tab.Panels>
          <Tab.Panel>
            {kioskId && <KioskItems kioskId={kioskId}></KioskItems>}
          </Tab.Panel>
          <Tab.Panel className="mt-12">
            {currentAccount && (
              <OwnedObjects address={currentAccount.address}></OwnedObjects>
            )}
          </Tab.Panel>
        </Tab.Panels>
      </Tab.Group>
    </div>
  );
}
