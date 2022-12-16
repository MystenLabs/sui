// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import '../styles/style.scss'
import Footer from '../components/Footer'
import Head from 'next/head'
import Main from '../components/Main'
import { WalletKitProvider } from "@mysten/wallet-kit";

function MyApp() {
  return (
    <>
      <WalletKitProvider>
        <Head>
          <title>Satoshi Multiplayer</title>
          <meta name="description" content="Satoshi Multiplayer game" />
        </Head>

        <main>
          <Main />
        </main>

        <footer className="bg-black">
          <Footer />
        </footer>
      </WalletKitProvider>
    </>
  )
}

export default MyApp
