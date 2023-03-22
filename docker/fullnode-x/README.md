
0. Build the containers locally to your desktop/laptop with:
   - `docker compose build`
      - Go to lunch or take a walk.

1. bring up postgres first so you can create the DB with the `diesel` commands:
	- `docker compose up postgres -d`
	  * **Note** that postgres will store its db data in `./postgres/data`
	- `psql -U postgres -p 5432 -h localhost -c 'create database sui_indexer_testnet'`
	- run these in sui.git/crates/sui-indexer:
      * `diesel setup --database-url=postgres://postgres:admin@localhost:5432/sui_indexer_testnet`

2. for whichever network you want to use, get the fullnode.yaml config and the genesis.blob files and place them in `fullnode/config/`

3. `docker compose up fullnode -d`
   - verify it's working by watching the logs with `docker compose logs fullnode -f`

4. Once the fullnode is working, then start indexer with:  `docker compose up indexer -d`
