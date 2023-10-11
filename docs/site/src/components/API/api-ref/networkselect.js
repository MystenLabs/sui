import React, { useState } from 'react';
import { Select, MenuItem, FormControl, InputLabel } from '@mui/material'

const NETWORKS = ["Devnet", "Testnet", "Mainnet"];

const NetworkSelect = () => {

    const [selection, setSelection] = useState('testnet');

    const handleChange = (e) => {
        setSelection(e.target.value);
    }

    return (
        <div className="w-11/12">
        <FormControl fullWidth>
            <InputLabel id="network" className="dark:text-white">{`RPC: https://fullnode.${selection.toLowerCase()}.sui.io:443`}</InputLabel>
            <Select 
                label-id="network"
                id="network-select"
                value={selection}
                label={`RPC: https://fullnode.${selection.toLowerCase()}.sui.io:443`}
                onChange={handleChange}
                className="dark:text-white"
            >
                <MenuItem value="devnet">Devnet</MenuItem>
                <MenuItem value="testnet">Testnet</MenuItem>
                <MenuItem value="mainnet">Mainnet</MenuItem>
            </Select>
        </FormControl>
        </div>
    )
}

export default NetworkSelect;