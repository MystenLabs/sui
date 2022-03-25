# Sui Explorer
Sui Explorer is a chain explorer for the Sui network, similiar in functionality to [Etherscan](https://etherscan.io/) or [Solana Explorer](https://explorer.solana.com/). Use Sui Explorer to see the latest blocks, transactions, and cluster statistics.

## Data source / RPC URL
The Sui Explorer front end can use any Sui RPC URL as a backend, but it must have Cross-Origin Resource Sharing (CORS) set up.

To change the RPC URL, pass a [url-encoded](https://developer.mozilla.org/en-US/docs/Glossary/percent-encoding) URL to the RPC parameter. So to use http://127.0.0.1:5000, the Sui REST API's default local port, use the URL:

http://127.0.0.1:3000?rpc=http%3A%2F%2F127.0.0.1%3A5000%2F

This defaults to https://demo-rpc.sui.io (for now).

The RPC URL preference will be remembered for three hours before being discarded - this way, it's not necessary to continually pass it on every page.

If the Sui Explorer is served over HTTPS, the RPC also needs to be HTTPS (proxied from HTTP).

## Running Locally

This is how you can run with a local REST server and local copy of the Sui Explorer.

Make sure you are in the `explorer-rest` project of the Sui repository, where this README resides.

### Getting started

You need [Rust](https://www.rust-lang.org/tools/install) & [Node.js](https://nodejs.org/en/download/) installed for this.

From the root of the `explorer-rest` project, run:

```bash
cargo build --release          # build the network and rest server
./target/release/rest_server        # run the rest server - defaults to port 5000
```

Now in another terminal tab, in another copy of the repo checked out to `experimental-rest-api`, run this from the repo root: 

```
cd explorer/client       
npm install              # install client dependencies
npm start                # start the development server for the client  
```

If everything worked, you should be able to view your local explorer at:
http://127.0.0.1:3000?rpc=http%3A%2F%2F127.0.0.1%3A5000%2F

### CORS issues / USE FIREFOX LOCALLY

Due to current technical issues, the REST API doesn't have CORS headers setup and must be proxied to add them. The demo server has this.

Chrome doesn't let you make requests without CORS on the local host, but **Firefox does**; so it is **strongly recommended to use Firefox for local testing**.

## ----------------------------------------------------------------------
## no more about running locally
## ----------------------------------------------------------------------

## Proposed basic architecture

This is described in order of how data flows, from the source network to the end user. Browser front end is a standard React app, powered by the HTTP API:

1. We get raw data from the network using the in-progress "bulk sync" API.
1. This raw data is dumped into a document store / noSQL kind of database (which one not decided yet).
1. A sync process runs to move new data from this raw cache into a more structured, relational database. (This DB is PostgreSQL, unless there's a strong argument for using something else presented.)
1. We index this database to optimize common queries: "all transactions for an address", "objects owned by address", etc. 
1. The HTTP API is implemented as a Rust webserver, talks to the relational database & encodes query results as JSON.

Plenty of the Cosmos explorers use the database step, citing how much easier it makes syncing history and recovering from periods of being offline. Access to untouched historical data means that any errors in converting the data into relational table form can be corrected later.

## Sub-projects

Front end code goes in `client` folder.

All back-end pieces (historical data store, relational DB, and HTTP layer) go in `server` sub-folders.
