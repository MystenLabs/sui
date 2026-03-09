// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect } from "react";
import ExecutionEnvironment from "@docusaurus/ExecutionEnvironment";
import RefNav from "./api-ref/refnav";
import CompNav from "./api-ref/compnav";
import Methods from "./api-ref/method";
import Components from "./api-ref/components";
import ScrollSpy from "react-ui-scrollspy";

export function getRef(url) {
  return url.substring(url.lastIndexOf("/") + 1);
}

const SPEC_URLS = {
  mainnet: "/mainnet/openrpc.json",
  testnet: "/testnet/openrpc.json",
  devnet: "/devnet/openrpc.json",
};

async function loadSpec(network) {
  const url = SPEC_URLS[network] || SPEC_URLS.mainnet;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`Failed to load ${url}: ${res.status}`);
  return res.json();
}

const Rpc = () => {
  const [openrpc, setOpenRpc] = useState(null);

  useEffect(() => {
    if (!ExecutionEnvironment.canUseDOM) return;

    const readNetwork = () => localStorage.getItem("RPC") || "mainnet";

    const rpcswitch = async () => {
      try {
        setOpenRpc(await loadSpec(readNetwork()));
      } catch (e) {
        console.error(e);
        // fallback
        try {
          setOpenRpc(await loadSpec("mainnet"));
        } catch (e2) {
          console.error(e2);
          setOpenRpc(null);
        }
      }
    };

    rpcswitch();
    window.addEventListener("storage", rpcswitch);
    return () => window.removeEventListener("storage", rpcswitch);
  }, []);

  if (!openrpc) return <p>Loading OpenRPC…</p>;

  const apis = [
    ...new Set((openrpc.methods || []).map((api) => api.tags?.[0]?.name)),
  ]
    .filter(Boolean)
    .sort();

  const schemas = openrpc.components?.schemas;

  return (
    <div className="mx-4 flex flex-row">
      <div className="pt-12 w-1/4 mb-24 flex-none max-h-screen overflow-y-auto sticky top-12">
        <RefNav json={openrpc} apis={apis} />
        <CompNav json={openrpc} apis={apis} />
      </div>

      <main className="flex-grow w-3/4">
        <div className="mx-8">
          <div className="">
            <h1 className="fixed bg-white dark:bg-ifm-background-color-dark w-full py-4 top-14">
              Sui JSON-RPC Reference - Version: {openrpc.info?.version}
            </h1>

            <div className="">
              <p className="pt-24">{openrpc.info?.description}</p>
              <ScrollSpy>
                <Methods json={openrpc} apis={apis} schemas={schemas} />
                <Components json={openrpc} apis={apis} schemas={schemas} />
              </ScrollSpy>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
};

export default Rpc;
