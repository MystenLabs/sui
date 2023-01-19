// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Outlet } from "react-router-dom";
import { Toaster } from "react-hot-toast";
import { WalletKitProvider } from "@mysten/wallet-kit";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import { Layout } from "../components/Layout";

export function Root() {
  return (
    <WalletKitProvider enableUnsafeBurner={import.meta.env.DEV}>
      <Layout>
        <Outlet />
      </Layout>
      <Toaster />
      {import.meta.env.DEV && <ReactQueryDevtools />}
    </WalletKitProvider>
  );
}
