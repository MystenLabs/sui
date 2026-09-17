// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#client
import { SuiGrpcClient } from '@mysten/sui/grpc';
import { predict } from '@mysten/deepbook-v3/predict';
import { FULLNODE_URL, NETWORK } from './config.js';

// `$extend` registers the Predict facade on the Sui client, so everything below
// reaches it at `client.predict`. Any client exposing the core API works,
// whether gRPC or JSON-RPC.
export const client = new SuiGrpcClient({
	network: NETWORK,
	baseUrl: FULLNODE_URL,
}).$extend(predict({ network: NETWORK }));

// Reads run through the client's own transaction simulation and object reads, so
// `client.predict.read.*` needs neither an indexer nor a Predict server.
//
// The SDK never signs and never holds keys. Every `client.predict.tx.*` builder
// returns a `Transaction` for a wallet, dapp-kit, or your own signer to execute.
// Execute with events included whenever you intend to decode a receipt, because
// the decoders read the events' canonical BCS bytes.
// docs::/#client
