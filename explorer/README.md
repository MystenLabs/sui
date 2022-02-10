# Sui Explorer
A chain explorer for the Sui network, similiar in functionality to [Etherscan](https://etherscan.io/) or [Solana Explorer](https://explorer.solana.com/).

## Proposed Basic Architecture

This is described in order of how data flows, from the source network to the end usder.

* We get raw data from the network using the in-progress "bulk sync" API

    This raw data is dumped into a document store / noSQL kind of DB (which one not decided yet).

    Plenty of the Cosmos explorers use this step, citing how much easier it makes syncing history & recovering from periods of being offline. Access to untouched historical data means that any errors in converting the data into relational table form can be corrected later.

* A sync process runs to move new data from this raw cache into a more structured, relational database. 

    This DB is PostgreSQL, unless there's a strong argument for using something else presented. 

    We index this database to optimize common queries - "all transactions for an address", "objects owned by address", etc. 

* The HTTP api is implemented as a Rust webserver, talks to the relational database & encodes query results as JSON

* Browser frontend is a standard React app, powered by the HTTP api


## Sub-Projects

Front-end code goes in `client` folder,

All back-end pieces (historical data store, relational DB, & HTTP layer) go in `server` sub-folders.