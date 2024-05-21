# Helm Charts
This directory contains helm charts to deploy Sui RPC2.0 infra. These charts are intended to be starting points for RPC2.0 providers. Not everything here is expected to work out of the box. Some of this infra has some scope/setup needed outside of these charts.


# Things of Note
## DB_URL Secret for indexers/graphql
For RPC2.0 services it's recommended that you create a K8's secret that contains the DB_URL. The indexer-reader/writer and graphql charts will assume a secret exists. Ensure the secret name matches that what is in the env section in the values.yaml file.

Example:
```'v: 
```
kubectl create secret generic db-secret \
  --from-literal=db-url='postgres://username:password@host:port/dbname'
  ```
# GraphQL

# Indexer-Reader

# Indexer-Writer
