
0. Build the containers locally to your desktop/laptop with:
   - `docker compose build`
      - The build process can take a while to complete.

1. First start postgres so you can create the DB with the `diesel` command:
	- `docker compose up postgres -d`
	  * **Note** that postgres will store its db data in `./postgres/data`
	- `psql -U postgres -p 5432 -h localhost -c 'create database sui_indexer_testnet'`
	- run these in sui.git/crates/sui-indexer:
      * `diesel setup --database-url=postgres://postgres:admin@localhost:5432/sui_indexer_testnet`

2. Copy the fullnode.yaml and genesis.blob files for the network to use and put them in the `fullnode/config/` folder.

3. `docker compose up fullnode -d`
   - verify it's working by watching the logs with `docker compose logs fullnode -f`

4. Once the full node is working, then start indexer with:  `docker compose up indexer -d`

	- You will see the indexer catching up checkpoint by checkpoint until it's up to date and ready to serve requests.
	  `docker compose logs indexer | tail -30`
