# sui-indexer-alt

## Set-up

Running the indexer requires a Postgres-compatible database to be installed and running on the
system.

### Postgres

#### Postgres 15 setup
1. Install postgres
   ```sh
   brew install postgresql@15
   ```
   Resulting in this connection string
   ```
   postgresql://$(whoami):postgres@localhost:5432/postgres
   ```

### AlloyDB Omni

#### Docker setup
1. Install docker
   ```sh
   brew install --cask docker
   ```

2. Run docker (requires password)
   ```sh
   open -a Docker
   ```

#### Option 1: AlloyDB setup for Docker
1. Run AlloyDB Omni in docker
   ```sh
   docker run --detach --publish 5433:5432 --env POSTGRES_PASSWORD=postgres_pw google/alloydbomni
   ```
   Resulting in this connection string
   ```
   postgresql://postgres:postgres_pw@localhost:5433/postgres
   ```

The indexer will try to connect to the following database URL by default:

```
postgres://postgres:postgrespw@localhost:5432/sui_indexer_alt
```

#### Option 2: AlloyDB setup for Kubernetes

1. Install Minikube
   ```sh
   brew install minikube
   ```

2. Create Kubernetes cluster
   ```sh
   minikube start --memory=16g
   ```
   Note: Cluster can be deleted with
   ```sh
   minikube delete
   ```
3. Install Helm
   ```sh
   brew install helm
   ```

4. Install cert-manager in the cluster
   ```sh
   helm install cert-manager oci://quay.io/jetstack/charts/cert-manager \
   --create-namespace \
   --namespace cert-manager \
   --set crds.enabled=true
   ```
   Example expected output
   ```
   NAME: cert-manager
   LAST DEPLOYED: Thu Oct 16 12:14:45 2025
   NAMESPACE: cert-manager
   STATUS: deployed
   REVISION: 1
   TEST SUITE: None
   ```
   Note: can be uninstalled with
   ```sh
   helm uninstall cert-manager -n cert-manager
   ```

5. Install Google Cloud CLI
   ```sh
   brew install --cask gcloud-cli
   ```

6. Login to Google Cloud
   ```sh
   gcloud auth login
   ```

7. Download AlloyDB Omni operator
   ```sh
   gcloud storage cp gs://alloydb-omni-operator/1.5.0/alloydbomni-operator-1.5.0.tgz ./ --recursive
   ```

8. Install AlloyDB Omni operator in the cluster
   ```sh
   helm install alloydbomni-operator alloydbomni-operator-1.5.0.tgz \
   --create-namespace \
   --namespace alloydb-omni-system
   ```
   Example expected output
   ```
   NAME: alloydbomni-operator
   LAST DEPLOYED: Thu Oct 16 12:37:56 2025
   NAMESPACE: alloydb-omni-system
   STATUS: deployed
   REVISION: 1
   TEST SUITE: None
   ```
   Note: can be uninstalled with
   ```sh
   helm uninstall alloydbomni-operator -n alloydb-omni-system
   ```

9. Install kubectl
   ```sh
   brew install kubectl
   ```

10. Apply AlloyDB Omni manifest
   ```sh
   echo "
apiVersion: v1
kind: Namespace
metadata:
  name: sui-indexer-alt
  labels:
    name: sui-indexer-alt
---
apiVersion: v1
kind: Secret
metadata:
  name: db-pw-sui-indexer-alt
  namespace: sui-indexer-alt
type: Opaque
data:
  sui-indexer-alt: $(echo password | base64)
---
apiVersion: alloydbomni.dbadmin.goog/v1
kind: DBCluster
metadata:
  name: sui-indexer-alt
  namespace: sui-indexer-alt
spec:
  databaseVersion: 16.8.0
  primarySpec:
    adminUser:
      passwordRef:
        name: db-pw-sui-indexer-alt
    resources:
      cpu: 1
      memory: 2Gi
      disks:
      - name: DataDisk
        size: 20Gi
" | kubectl apply -f -
   ```
   Expected output
   ```
   secret/db-pw-sui-indexer-alt created
   dbcluster.alloydbomni.dbadmin.goog/sui-indexer-alt created
   ```

This database can be created with the following commands (run from this directory):

```sh
# Install the CLI (if not already installed)
cargo install diesel_cli --no-default-features --features postgres

# Use it to create the database and run migrations on it.
diesel setup                                                                       \
    --database-url="postgres://postgres:postgrespw@localhost:5432/sui_indexer_alt" \
    --migration-dir ../sui-indexer-alt-schema/migrations
```

If more migrations are added after the database is set-up, the indexer will
automatically apply them when it starts up.

## Tests

Tests require postgres to be installed (but not necessarily running), and
benefit from the following tools:

```sh
cargo install cargo-insta   # snapshot testing utility
cargo install cargo-nextest # better test runner
```

The following tests are related to the indexer (**run from the root of the
repo**):

```sh
cargo nextest run                \
    -p sui-indexer-alt           \
    -p sui-indexer-alt-framework \
    -p sui-indexer-alt-e2e-tests
```

The first package is the indexer's own unit tests, the second is the indexing
framework's unit tests, and the third is an end-to-end test suite that includes
the indexer as well as the RPCs that read from its database.

## Configuration

The indexer is mostly configured through a TOML file, a copy of the default
config can be generated using the following command:

```sh
cargo run --bin sui-indexer-alt -- generate-config > indexer_alt_config.toml
```

## Running
A source of checkpoints is required (exactly one of `--remote-store-url`,
`--local-ingestion-path`, or `--rpc-api-url`), and a `--config` must be
supplied (see "Configuration" above for details on generating a configuration
file).

```sh
cargo run --bin sui-indexer-alt -- indexer               \
  --database-url {url}                                   \
  --remote-store-url https://checkpoints.mainnet.sui.io  \
  --config indexer_alt_config.toml
```

## Pruning

Some pipelines identify regions to prune by transaction sequence number, or by
epoch. These pipelines require the `cp_sequence_numbers` table be populated in
the database they are writing to, otherwise they are unable to translate a
checkpoint sequence range into a transaction or epoch range.

Only one instance of the indexer writing to that database needs to populate
this table, by enabling the `cp_sequence_numbers` pipeline.
