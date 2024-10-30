// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect } from "react";
import ExecutionEnvironment from "@docusaurus/ExecutionEnvironment";
import RefNav from "./api-ref/refnav";
import CompNav from "./api-ref/compnav";
import Methods from "./api-ref/method";
import Components from "./api-ref/components";

import ScrollSpy from "react-ui-scrollspy";

import openrpc_mainnet from "../../open-spec/mainnet/openrpc.json";
import openrpc_testnet from "../../open-spec/testnet/openrpc.json";
import openrpc_devnet from "../../open-spec/devnet/openrpc.json";

export function getRef(url) {
  return url.substring(url.lastIndexOf("/") + 1, url.length);
}

const Rpc = () => {
  const [openrpc, setOpenRpc] = useState(() => {
    if (ExecutionEnvironment.canUseDOM) {
      const network = localStorage.getItem("RPC");
      switch (network) {
        case "mainnet":
          return openrpc_mainnet;
        case "testnet":
          return openrpc_testnet;
        case "devnet":
          return openrpc_devnet;
        default:
          return openrpc_mainnet;
      }
    } else {
      return openrpc_mainnet;
    }
  });

  useEffect(() => {
    const rpcswitch = () => {
      if (localStorage.getItem("RPC")) {
        switch (localStorage.getItem("RPC")) {
          case "mainnet":
            setOpenRpc(openrpc_mainnet);
            break;
          case "testnet":
            setOpenRpc(openrpc_testnet);
            break;
          case "devnet":
            setOpenRpc(openrpc_devnet);
            break;
          default:
            setOpenRpc(openrpc_mainnet);
        }
      } else {
        setOpenRpc(openrpc_mainnet);
      }
    };

    window.addEventListener("storage", rpcswitch);

    return () => {
      window.removeEventListener("storage", rpcswitch);
    };
  }, [openrpc]);

  const apis = [
    ...new Set(openrpc["methods"].map((api) => api.tags[0].name)),
  ].sort();
  const schemas = openrpc.components.schemas;

  if (!openrpc) {
    return <p>Open RPC file not found.</p>;
  }

  let ids = [];
  openrpc["methods"].map((method) => {
    ids.push(method.name.replaceAll(/\s/g, "-").toLowerCase());
  });

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
              Sui JSON-RPC Reference - Version: {openrpc.info.version}
            </h1>
            
              <div className="">
                <p className="pt-24">{openrpc.info.description}</p>
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
