// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const axios = require('axios');
const fs = require('fs');
const path = require('path');

// Download Mainnet OpenRPC spec
axios({
    method: "get",
    url: "https://raw.githubusercontent.com/MystenLabs/sui/mainnet/crates/sui-open-rpc/spec/openrpc.json",
    responseType: "blob"
}).then((res) => {

    if (fs.existsSync(path.join(__dirname, "../open-spec/mainnet/openrpc_backup.json"))){
        fs.unlink(path.join(__dirname, "../open-spec/mainnet/openrpc_backup.json"), (err) => {
            if (err) {
                return console.log(err);
            }
            console.log("Deleted backup spec.")
        } )
    } else {
        console.log("Backup file does not exist.")
    }
    if (fs.existsSync(path.join(__dirname, "../open-spec/mainnet/openrpc.json"))){
        fs.renameSync(path.join(__dirname, "../open-spec/mainnet/openrpc.json"), path.join(__dirname, "../open-spec/mainnet/openrpc_backup.json"));
    }
    
    fs.writeFileSync(path.join(__dirname, "../open-spec/mainnet/openrpc.json"), res.data, 'utf8');
}).catch(err => {
    console.log("Error downloading Mainnet openrpc spec.");
    console.error(err);
})

// Download Testnet OpenRPC spec
axios({
    method: "get",
    url: "https://raw.githubusercontent.com/MystenLabs/sui/testnet/crates/sui-open-rpc/spec/openrpc.json",
    responseType: "blob"
}).then((res) => {

    if (fs.existsSync(path.join(__dirname, "../open-spec/testnet/openrpc_backup.json"))){
        fs.unlink(path.join(__dirname, "../open-spec/testnet/openrpc_backup.json"), (err) => {
            if (err) {
                return console.log(err);
            }
            console.log("Deleted backup spec.")
        } )
    } else {
        console.log("Backup file does not exist.")
    }
    if (fs.existsSync(path.join(__dirname, "../open-spec/testnet/openrpc.json"))){
        fs.renameSync(path.join(__dirname, "../open-spec/testnet/openrpc.json"), path.join(__dirname, "../open-spec/testnet/openrpc_backup.json"));
    }
    
    fs.writeFileSync(path.join(__dirname, "../open-spec/testnet/openrpc.json"), res.data, 'utf8');
}).catch(err => {
    console.log("Error downloading Testnet openrpc spec.");
    console.error(err);
})

// Download Devnet OpenRPC spec
axios({
    method: "get",
    url: "https://raw.githubusercontent.com/MystenLabs/sui/devnet/crates/sui-open-rpc/spec/openrpc.json",
    responseType: "blob"
}).then((res) => {

    if (fs.existsSync(path.join(__dirname, "../open-spec/devnet/openrpc_backup.json"))){
        fs.unlink(path.join(__dirname, "../open-spec/devnet/openrpc_backup.json"), (err) => {
            if (err) {
                return console.log(err);
            }
            console.log("Deleted backup spec.")
        } )
    } else {
        console.log("Backup file does not exist.")
    }
    if (fs.existsSync(path.join(__dirname, "../open-spec/devnet/openrpc.json"))){
        fs.renameSync(path.join(__dirname, "../open-spec/devnet/openrpc.json"), path.join(__dirname, "../open-spec/devnet/openrpc_backup.json"));
    }
    
    fs.writeFileSync(path.join(__dirname, "../open-spec/devnet/openrpc.json"), res.data, 'utf8');
}).catch(err => {
    console.log("Error downloading Devnet openrpc spec.");
    console.error(err);
})