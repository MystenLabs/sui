// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect } from "react";
import ExecutionEnvironment from "@docusaurus/ExecutionEnvironment";
import { Select, MenuItem, FormControl, InputLabel } from "@mui/material";
import { StyledEngineProvider } from "@mui/material/styles";

const NETWORKS = ["Devnet", "Testnet", "Mainnet"];

const NetworkSelect = () => {
  const [selection, setSelection] = useState(() => {
    if (ExecutionEnvironment.canUseDOM) {
      const network = localStorage.getItem("RPC");
      if (network === null) {
        return "mainnet";
      }
      return localStorage.getItem("RPC");
    } else {
      return "mainnet";
    }
  });

  useEffect(() => {
    localStorage.setItem("RPC", selection);
    window.dispatchEvent(new Event("storage"));
  }, [selection]);

  const handleChange = (e) => {
    setSelection(e.target.value);
  };

  return (
    <StyledEngineProvider injectFirst>
      <div className="w-11/12">
        <FormControl fullWidth>
          <InputLabel
            id="network"
            className="dark:text-white"
          >{`RPC: https://fullnode.${selection.toLowerCase()}.sui.io:443`}</InputLabel>
          <Select
            label-id="network"
            id="network-select"
            value={selection}
            label={`RPC: https://fullnode.${selection.toLowerCase()}.sui.io:443`}
            onChange={handleChange}
            className="dark:text-white dark:bg-sui-ghost-dark"
          >
            <MenuItem value="devnet">{NETWORKS[0]}</MenuItem>
            <MenuItem value="testnet">{NETWORKS[1]}</MenuItem>
            <MenuItem value="mainnet">{NETWORKS[2]}</MenuItem>
          </Select>
        </FormControl>
      </div>
    </StyledEngineProvider>
  );
};

export default NetworkSelect;
