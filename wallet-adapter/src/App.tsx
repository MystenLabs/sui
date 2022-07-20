// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from 'react';
import logo from './logo.svg';
import './App.css';
import { root } from '.';
import { ConnectWalletModal } from 'sui-wallet-adapter-ui';
import { Wallet, WalletProvider } from 'sui-wallet-adapter-react';
import { SuiWalletAdapter, MockWalletAdapter} from '@sui-wallet-adapter/all-wallets';
import { ManageWalletModal } from 'sui-wallet-adapter-ui';
import { WalletWrapper } from 'sui-wallet-adapter-ui';
import { Button } from '@mui/material';
import { TestButton } from './TestButton';

function App() {
  const supportedWallets: Wallet[] = [
    {
      adapter: new SuiWalletAdapter()
    },
    {
      adapter: new MockWalletAdapter("Ethos Wallet")
    },
  ];

  return (
    <div className="App">
      <header className="App-header">
        <WalletProvider supportedWallets={supportedWallets}>
            <TestButton/>
            <br/>
            <WalletWrapper/>
        </WalletProvider>
      </header>
    </div>
  );
}

export default App;