// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Outlet } from "react-router-dom";
import { Toaster } from "react-hot-toast";
import { WalletKitProvider } from "@mysten/wallet-kit";
import { ReactQueryDevtools } from "@tanstack/react-query-devtools";
import { Layout } from "../components/Layout";

export function Root() {
  return (
    <WalletKitProvider>
      <Layout>
        <Outlet />
      </Layout>

      <Toaster
        position="bottom-center"
        gutter={8}
        containerStyle={{
          top: 40,
          left: 40,
          bottom: 40,
          right: 40,
        }}
        toastOptions={{
          duration: 4000,
          success: {
            icon: null,
            className: "!bg-success-light !text-success-dark",
          },
          error: {
            icon: null,
            className: "!bg-issue-light !text-issue-dark",
          },
          style: {
            wordBreak: "break-word",
            maxWidth: 500,
          },
        }}
      />
      {import.meta.env.DEV && <ReactQueryDevtools />}
    </WalletKitProvider>
  );
}
