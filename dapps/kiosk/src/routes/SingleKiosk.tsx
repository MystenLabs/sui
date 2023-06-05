// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNavigate, useParams } from 'react-router-dom';
import { KioskItems } from '../components/Kiosk/KioskItems';
import { useWalletKit } from '@mysten/wallet-kit';

export default function SingleKiosk(): JSX.Element {
  const { kioskId } = useParams();
  const navigate = useNavigate();

  const { currentAccount } = useWalletKit();

  if (!kioskId) {
    navigate('/');
    return <></>;
  }

  return (
    <div className="container">
      <KioskItems
        kioskId={kioskId}
        address={currentAccount?.address}
      ></KioskItems>
    </div>
  );
}
