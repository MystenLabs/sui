// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from '@mysten/wallet-kit';
import { useRpc } from '../hooks/useRpc';
import { KIOSK_OWNER_CAP, createKioskAndShare } from '@mysten/kiosk';
import { useEffect, useState } from 'react';
import { TransactionBlock, getObjectFields, getObjectId } from '@mysten/sui.js';
import { useTransactionExecution } from '../hooks/useTransactionExecution';
import { KioskData } from '../components/Kiosk/KioskData';
import { Loading } from '../components/Base/Loading';
import { localStorageKeys } from '../utils/utils';
import { SuiConnectButton } from '../components/Base/SuiConnectButton';

function Home() {
  const { currentAccount } = useWalletKit();
  const [kioskIds, setKioskIds] = useState<string[]>([]);
  const [loadingCap, setLoadingCap] = useState<boolean>(false);
  const { signAndExecute } = useTransactionExecution();
  const [kioskId, setKioskId] = useState<string | null>(null);
  const provider = useRpc();

  const findUserKiosks = async () => {
    if (!currentAccount?.address) return;

    setLoadingCap(true);

    // get kiosk owner Cap objects
    const kiosks = await provider.getOwnedObjects({
      owner: currentAccount.address,
      filter: { StructType: `${KIOSK_OWNER_CAP}` },
      options: {
        showContent: true,
      },
    });

    const kioskIdList = kiosks?.data?.map((x) => getObjectFields(x)?.for);
    setKioskIds(kioskIdList);

    const kioskOwnerCaps = kiosks?.data.map((x) => getObjectId(x));

    // save to localStorage for easy retrieval throughout the app.
    localStorage.setItem(localStorageKeys.USER_KIOSK_ID, kioskIdList[0]);
    localStorage.setItem(
      localStorageKeys.USER_KIOSK_OWNER_CAP,
      kioskOwnerCaps[0],
    );
    setKioskId(kioskIdList[0]);
    setLoadingCap(false);
  };

  const createNewKiosk = async () => {
    if (!currentAccount?.address) return;

    const tx = new TransactionBlock();
    const kiosk_cap = createKioskAndShare(tx);

    tx.transferObjects(
      [kiosk_cap],
      tx.pure(currentAccount?.address, 'address'),
    );

    await signAndExecute({ tx });
    findUserKiosks();
  };

  useEffect(() => {
    if (!currentAccount?.address) {
      setKioskIds([]);
      setKioskId(null);
      localStorage.removeItem(localStorageKeys.USER_KIOSK_ID);
    }
    findUserKiosks();
  }, [currentAccount?.address]);

  return (
    <div className="container">
      <div>
        {!kioskId && (
          <div className=" mb-12 flex items-center justify-center">
            <div>
              {!currentAccount?.address && (
                <div className="flex justify-center min-h-[70vh] items-center">
                  <div className="text-center">
                    <div>
                      <h2 className="font-bold text-2xl">
                        Connect your wallet to manage your kiosk
                      </h2>
                      <p className="pb-6 pt-3">
                        Create your kiosk to manage your kiosk and <br />
                        purchase from other kiosks.
                      </p>
                    </div>
                    <SuiConnectButton />
                  </div>
                </div>
              )}

              {loadingCap && <Loading />}

              {!loadingCap && (
                <>
                  {kioskIds.length > 0 && (
                    <div className="mb-20 mt-6 text-center">
                      <button
                        onClick={() => {
                          setKioskId(kioskIds[0]);
                        }}
                      >
                        View Kiosk
                      </button>
                    </div>
                  )}

                  {currentAccount && kioskIds.length < 1 && (
                    <div className="min-h-[70vh] flex items-center justify-center gap-4 mt-6 text-center">
                      <div>
                        <h2 className="font-bold text-2xl">
                          You don't have a kiosk yet.
                        </h2>
                        <p>Create your kiosk to start trading.</p>
                        <button onClick={createNewKiosk} className="mt-8">
                          Create your Kiosk
                        </button>
                      </div>
                    </div>
                  )}
                </>
              )}
            </div>
          </div>
        )}

        {kioskId && currentAccount?.address && (
          <KioskData setSelectedKiosk={setKioskId} />
        )}
      </div>
    </div>
  );
}

export default Home;
