// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * This package generates test data for the Axelar
 * General Message Passing protocol
 */

const { fromHEX, toHEX } = require("@mysten/bcs");
const secp256k1 = require("secp256k1");
const keccak256 = require("keccak256");
const bcs = require("./bcs");

/**
 * Generate a set of 10 private keys to use in tests for operator rotation.
 * This is a TEST ONLY function to provide data for testing. Not to
 * be used in production. Private keys should never be stored in any
 * repository.
 */
function generateKeys() {
  const { randomBytes } = require('crypto');
  const operators = [];
  for (let i = 0; i < 10; i++) {
    operators.push(randomBytes(32).toString('hex'));
  }

  console.log(JSON.stringify(operators, null, 4));
}

// uncomment when regen needed
// generateKeys(); process.exit(0);

/**
 * Read locally stored keys (requires running generateKeys) and
 * generate public keys + secp256k1 wrappers around it.
 */
function readOperators() {
  const pks = require('./../operators.keys.json');
  const operators = pks.map((pk) => ({
    weight: 50,
    privKey: pk,
    pubKey: secp256k1.publicKeyCreate(fromHEX(pk)),
    sign: (msg) => secp256k1.ecdsaSign(msg, fromHEX(pk))
  }));

  return operators;
}

/**
 * Stores read operators.
 * Used for signing on messages.
 */
const OPERATORS = readOperators();

/**
 * Generates a set of Base16-encoded addresses
 * to set in the `messenger::Axelar` object.
 */
function getAxelarInitAddresses() {
  return OPERATORS.map((op) => toHEX(op.pubKey));
}

// uncomment when init data needs to be set
// console.log(JSON.stringify(getAxelarInitAddresses(), null, 2)); process.exit();

const ZERO_ADDR = "0x".padEnd(62, "0");

/**
 * Create a new AxelarMessage.
 * Supports passing pre-serialized messages along with their IDs.
 */
function createSignedAxelarMessage(commandIds, commands) {
  // Create message to sign it later
  const message = bcs.ser('AxelarMessage', {
    chain_id: 1,
    command_ids: commandIds,
    commands: commandIds.map(() => "approveContractCall"),
    params: commands
  });

  // Hash the message with the SUI prefix
  const hashed = fromHEX(hashMessage(message.toBytes()));

  // Proof consists of Operator signatures on the message
  const proof = bcs.ser('Proof', {
    operators: OPERATORS.map((op) => op.pubKey),
    weights: OPERATORS.map((op) => op.weight),
    threshold: 10,
    signatures: OPERATORS.map((op) => {
      const { signature, recid } = op.sign(hashed);
      return new Uint8Array([...signature, recid]);
    })
  }, 1024 * 5);

  // Serialize everything into a single TX argument
  return bcs.ser('Input', {
    data: message.toBytes(),
    proof: proof.toBytes()
  }, 1025 * 5).toString('hex');
}

/**
 * Generate a simple single-message `Input`.
 */
function testExecuteInput() {
return createSignedAxelarMessage([ 'beep_boop' ], [
    bcs.ser('GenericMessage', {
      source_chain: "ETH",
      source_address: "0x0",
      payload_hash: [0,0,0,0],
      target_id: ZERO_ADDR,
      payload: [0, 0, 0, 0, 0]
    }).toBytes()
  ]);
}

// uncomment when needed; generates test data for a simple execute test
console.log(JSON.stringify(testExecuteInput(), null, 2)); process.exit(0);


// bcs
//   .ser("GenericMessage", {
//     source_chain: "ETH",
//     source_address: "0x0",
//     payload_hash: [0, 0, 0, 0],
//     target_id: ZERO_ADDR, // using address here for simlicity...
//     payload: [0, 0, 0, 0, 0],
//   })
//   .toBytes(),
// bcs
//   .ser("GenericMessage", {
//     source_chain: "AXELAR",
//     source_address: "0x1",
//     payload_hash: [0, 0, 0, 0],
//     target_id: ZERO_ADDR, // ...
//     payload: [0, 0, 0, 0, 0],
//   })
//   .toBytes(),



// console.log("OPERATOR: %s", toHEX(pubKey));
// console.log("DATA LENGTH: %d", message.length);
// console.log("PROOF LENGTH: %d", proof.length);
// console.log("INPUT: %s", input + "00");

// // verify the signature // just to make sure that everything is correct on this end
// console.log(secp256k1.ecdsaVerify(signed_data, hashed, pubKey));

// {
//   const testData = fromHEX(hashMessage("hello world bee"));
//   const { recid, signature } = secp256k1.ecdsaSign(testData, privKey);

//   // don't forget to add '00' to the end of the signature
//   console.log("message", hashMessage("hello world").replace("0x", ""));
//   console.log("signature", toHEX(signature) + recid.toString(16).padStart(2, 0));
//   console.log(secp256k1.ecdsaVerify(signature, testData, pubKey));
//   // console.log(secp256k1.ecdsaRecover(signature, recid, testData));
// }

/**
 * Add a prefix to a message.
 * Return resulting array of bytes.
 */
function hashMessage(data) {
  // sorry for putting it here...
  const messagePrefix = new Uint8Array(
    Buffer.from("\x19Sui Signed Message:\n", "ascii")
  );
  let hashed = new Uint8Array(messagePrefix.length + data.length);
  hashed.set(messagePrefix);
  hashed.set(data, messagePrefix.length);

  return keccak256(Buffer.from(hashed)).toString('hex');
}
