This example builds a basic indexer. You need to follow the [online instructions](https://docs.sui.io/guides/developer/advanced/custom-indexer2) to get the indexer to function. If you have the set up complete already (including a valid migrations directory), you just need to add a .env file with the following postgres URL information (update `username`).

```
DATABASE_URL=postgres://username@localhost:5432/sui_indexer
```
