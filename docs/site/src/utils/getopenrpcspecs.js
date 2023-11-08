// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const axios = require('axios');
const fs = require('fs');
const path = require('path');

// Create directory

const topdir = path.join(__dirname, "../open-spec");
const mainnetdir = path.join(topdir, "mainnet");
const testnetdir = path.join(topdir, "testnet");
const devnetdir = path.join(topdir, "devnet");

if (!fs.existsSync(topdir)){
    fs.mkdirSync(topdir);
}

const downloadFile = (url, rpcpath) => {
    if (!fs.existsSync(rpcpath)){
        fs.mkdirSync(rpcpath);
    }
    axios({
        method: "get",
        url,
        responseType: "blob"
    }).then((res) => {
    
        if (fs.existsSync(path.join(rpcpath, "openrpc_backup.json"))){
            fs.unlink(path.join(rpcpath, "openrpc_backup.json"), (err) => {
                if (err) {
                    return console.log(err);
                }
                console.log("Deleted backup spec.")
            } )
        } else {
            console.log("Backup file does not exist.")
        }
        if (fs.existsSync(path.join(rpcpath, "openrpc.json"))){
            fs.renameSync(path.join(rpcpath, "openrpc.json"), path.join(rpcpath, "openrpc_backup.json"));
        }
        
        fs.writeFileSync(path.join(rpcpath, "openrpc.json"), res.data, 'utf8');
    }).catch(err => {
        console.log("Error downloading openrpc spec.");
        console.error(err);
    })
}

// Download Mainnet OpenRPC spec
downloadFile("https://raw.githubusercontent.com/MystenLabs/sui/mainnet/crates/sui-open-rpc/spec/openrpc.json", mainnetdir);

// Download Testnet OpenRPC spec
downloadFile("https://raw.githubusercontent.com/MystenLabs/sui/testnet/crates/sui-open-rpc/spec/openrpc.json", testnetdir);

// Download Devnet OpenRPC spec
downloadFile("https://raw.githubusercontent.com/MystenLabs/sui/devnet/crates/sui-open-rpc/spec/openrpc.json", devnetdir);