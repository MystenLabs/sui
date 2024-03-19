// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const axios = require('axios');
const fs = require('fs');
const path = require('path');

// Create directory

const topdir = path.join(__dirname, "../open-spec");

if (!fs.existsSync(topdir)){
    fs.mkdirSync(topdir);
}

const downloadFile = (branch) => {
    const branchdir = path.join(topdir,branch);
    if (!fs.existsSync(branchdir)){
        fs.mkdirSync(branchdir);
    }
    axios({
        method: "get",
        url: `https://raw.githubusercontent.com/MystenLabs/sui/${branch}/crates/sui-open-rpc/spec/openrpc.json`,
        responseType: "blob"
    }).then((res) => {
        if (fs.existsSync(path.join(__dirname, `../open-spec/${branch}/openrpc_backup.json`))){
            fs.unlink(path.join(__dirname, `../open-spec/${branch}/openrpc_backup.json`), (err) => {
                if (err) {
                    return console.log(err);
                }
                console.log(`Deleted ${branch} backup spec.`)
            } )
        } else {
            console.log(`Backup file for ${branch} does not exist.`)
        }
        if (fs.existsSync(path.join(__dirname, `../open-spec/${branch}/openrpc.json`))){
            fs.renameSync(path.join(__dirname, `../open-spec/${branch}/openrpc.json`), path.join(__dirname, `../open-spec/${branch}/openrpc_backup.json`));
        }
        fs.writeFileSync(path.join(__dirname, `../open-spec/${branch}/openrpc.json`), res.data, 'utf8');
    }).catch(err => {
        console.log(`Error downloading ${branch} openrpc spec.`);
        console.error(err);
    })
}

// Download Mainnet OpenRPC spec
downloadFile("mainnet");

// Download Testnet OpenRPC spec
downloadFile("testnet");

// Download Devnet OpenRPC spec
downloadFile("devnet");