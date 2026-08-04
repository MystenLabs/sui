// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getJsonRpcFullnodeUrl, SuiJsonRpcClient } from "@mysten/sui/jsonRpc";
import { Ed25519Keypair } from "@mysten/sui/keypairs/ed25519";
import { Transaction } from "@mysten/sui/transactions";

const client = new SuiJsonRpcClient({
  url: getJsonRpcFullnodeUrl("testnet"),
  network: "testnet",
});

// docs::#generate-new
const keypair = new Ed25519Keypair();
const address = keypair.toSuiAddress();

// Log the address only. Never log getSecretKey() — stdout is captured by most
// log pipelines. Write the secret directly to your KMS or secrets manager.
console.log("Agent address:", address);
// docs::/#generate-new

// docs::#from-mnemonic
const mnemonic = "word1 word2 word3 ... word12";
const mnemonicKeypair = Ed25519Keypair.deriveKeypair(mnemonic);
// docs::/#from-mnemonic

// docs::#from-secret-key
// fromSecretKey accepts the Bech32 suiprivkey... string directly.
const secretKeyKeypair = Ed25519Keypair.fromSecretKey("suiprivkey1...");
// docs::/#from-secret-key

// docs::#env-load
function loadAgentKeypair(): Ed25519Keypair {
  const secretKey = process.env.AGENT_SECRET_KEY;

  if (!secretKey) {
    throw new Error("AGENT_SECRET_KEY is not set");
  }

  return Ed25519Keypair.fromSecretKey(secretKey);
}

const envKeypair = loadAgentKeypair();
// docs::/#env-load

// docs::#derive-address
const agentAddress = keypair.toSuiAddress();
// docs::/#derive-address

// docs::#sign-execute
const tx = new Transaction();
tx.setSender(agentAddress);
tx.setGasPayment([]); // Pay gas from the agent's SUI address balance.
tx.moveCall({ target: "0xPACKAGE::module::function" });

// Sign and execute in one step
const result = await client.signAndExecuteTransaction({
  transaction: tx,
  signer: keypair,
});
// docs::/#sign-execute

// docs::#sign-sponsored
const sponsoredTx = new Transaction();
sponsoredTx.setSender(agentAddress);
sponsoredTx.moveCall({ target: "0xPACKAGE::module::function" });

// Building with a client sets the expiration to the current epoch + 1 by
// default. If the gas station may hold these bytes across an epoch boundary,
// widen or disable the expiration explicitly:
//   sponsoredTx.setExpiration({ None: true });
const bytes = await sponsoredTx.build({ client });
const { signature } = await keypair.signTransaction(bytes);
// docs::/#sign-sponsored

// docs::#key-rotation
// Payment assets stay in address balances and move with balance::send_funds.
// Transfer separately owned capabilities and mandates as objects.
async function rotateAgentObjects(
  oldKeypair: Ed25519Keypair,
  newAddress: string,
) {
  const rotateTx = new Transaction();
  rotateTx.setSender(oldKeypair.toSuiAddress());

  for (const objectId of ["0xMANDATE_OBJECT_ID", "0xCAP_OBJECT_ID"]) {
    rotateTx.transferObjects([rotateTx.object(objectId)], newAddress);
  }

  return client.signAndExecuteTransaction({
    transaction: rotateTx,
    signer: oldKeypair,
  });
}
// docs::/#key-rotation

export { loadAgentKeypair, rotateAgentObjects };
