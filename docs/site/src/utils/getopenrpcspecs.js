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

if (!fs.existsSync(mainnetdir)){
    fs.mkdirSync(mainnetdir);
}

if (!fs.existsSync(testnetdir)){
    fs.mkdirSync(testnetdir);
}

if (!fs.existsSync(devnetdir)){
    fs.mkdirSync(devnetdir);
}

// Download Mainnet OpenRPC spec
axios({
    method: "get",
    url: "https://raw.githubusercontent.com/MystenLabs/sui/mainnet/crates/sui-open-rpc/spec/openrpc.json",
    responseType: "blob"
}).then((res) => {

    if (fs.existsSync(path.join(mainnetdir, "openrpc_backup.json"))){
        fs.unlink(path.join(mainnetdir, "openrpc_backup.json"), (err) => {
            if (err) {
                return console.log(err);
            }
            console.log("Deleted backup spec.")
        } )
    } else {
        console.log("Backup file does not exist.")
    }
    if (fs.existsSync(path.join(mainnetdir, "openrpc.json"))){
        fs.renameSync(path.join(mainnetdir, "openrpc.json"), path.join(mainnetdir, "openrpc_backup.json"));
    }
    
    fs.writeFileSync(path.join(mainnetdir, "openrpc.json"), res.data, 'utf8');
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

    if (fs.existsSync(path.join(testnetdir, "openrpc_backup.json"))){
        fs.unlink(path.join(testnetdir, "openrpc_backup.json"), (err) => {
            if (err) {
                return console.log(err);
            }
            console.log("Deleted backup spec.")
        } )
    } else {
        console.log("Backup file does not exist.")
    }
    if (fs.existsSync(path.join(testnetdir, "openrpc.json"))){
        fs.renameSync(path.join(testnetdir, "openrpc.json"), path.join(testnetdir, "openrpc_backup.json"));
    }
    
    fs.writeFileSync(path.join(testnetdir, "openrpc.json"), res.data, 'utf8');
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

    if (fs.existsSync(path.join(devnetdir, "openrpc_backup.json"))){
        fs.unlink(path.join(devnetdir, "openrpc_backup.json"), (err) => {
            if (err) {
                return console.log(err);
            }
            console.log("Deleted backup spec.")
        } )
    } else {
        console.log("Backup file does not exist.")
    }
    if (fs.existsSync(path.join(devnetdir, "openrpc.json"))){
        fs.renameSync(path.join(devnetdir, "openrpc.json"), path.join(devnetdir, "openrpc_backup.json"));
    }
    
    fs.writeFileSync(path.join(devnetdir, "openrpc.json"), res.data, 'utf8');
}).catch(err => {
    console.log("Error downloading Devnet openrpc spec.");
    console.error(err);
})