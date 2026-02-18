// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect } from "react";
import ExecutionEnvironment from "@docusaurus/ExecutionEnvironment";
import { Select, MenuItem, FormControl, InputLabel, FormHelperText } from "@mui/material";
import { StyledEngineProvider } from "@mui/material/styles";

const NETWORKS = [
  { label: "Devnet", value: "devnet" },
  { label: "Testnet", value: "testnet" },
  { label: "Mainnet", value: "mainnet" },
];

const NetworkSelect = () => {
  const [selection, setSelection] = useState(() => {
    if (ExecutionEnvironment.canUseDOM) {
      return localStorage.getItem("RPC") ?? "mainnet";
    }
    return "mainnet";
  });

  useEffect(() => {
    localStorage.setItem("RPC", selection);
    window.dispatchEvent(new Event("storage"));
  }, [selection]);

  const rpcUrl = `https://fullnode.${selection}.sui.io:443`;

  return (
    <StyledEngineProvider injectFirst>
      <div className="w-full">
        <FormControl fullWidth size="small">
          <InputLabel id="network">Network</InputLabel>
          <Select
            labelId="network"
            id="network-select"
            value={selection}
            label="Network"
            onChange={(e) => setSelection(e.target.value)}
            className="dark:text-white dark:bg-sui-ghost-dark"
          >
            {NETWORKS.map((n) => (
              <MenuItem key={n.value} value={n.value}>
                {n.label}
              </MenuItem>
            ))}
          </Select>
          <FormHelperText className="api-muted">
            RPC: <span className="api-typechip">{rpcUrl}</span>
          </FormHelperText>
        </FormControl>
      </div>
    </StyledEngineProvider>
  );
};

export default NetworkSelect;
