// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ConnectButton, useWalletKit } from '@mysten/wallet-kit';
import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

export function SuiConnectButton() {
  const { currentAccount } = useWalletKit();
  // redirect user to home page if they switch accounts / disconnect, to keep state management easier.
  // In a real dapp scenario, we'd want to use tanstack or an app state solution
  // to keep track of these and refetch owned kiosk.
  const navigate = useNavigate();
  useEffect(() => {
    navigate('/');
  }, [currentAccount?.address]);

  return (
    <ConnectButton
      style={{
        backgroundColor: '#101827',
      }}
    />
  );
}
