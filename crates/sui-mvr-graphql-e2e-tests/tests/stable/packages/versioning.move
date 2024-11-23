// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 P1=0x0 P2=0x0 --accounts A --simulator

//# publish --upgradeable --sender A
module P0::m {
    public fun f(): u64 { 42 }
}

//# create-checkpoint

//# run-graphql
{
    latestPackage(address: "@{P0}") {
        version
        module(name: "m") {
            functions { nodes { name } }
        }

        packageVersions {
            nodes {
                address
                version
            }
        }
    }

    firstPackage: package(address: "@{P0}", version: 1) {
        address
        version
        module(name: "m") {
            functions { nodes { name } }
        }

        packageVersions {
            nodes {
                address
                version
            }
        }
    }

    packages(first: 10) {
        nodes {
            address
            version
        }
    }
}

//# upgrade --package P0 --upgrade-capability 1,1 --sender A
module P1::m {
    public fun f(): u64 { 42 }
    public fun g(): u64 { 43 }
}

//# create-checkpoint

//# run-graphql
{
    latestPackage(address: "@{P0}") {
        version
        module(name: "m") {
            functions { nodes { name } }
        }

        packageVersions {
            nodes {
                address
                version
            }
        }
    }

    firstPackage: package(address: "@{P1}", version: 1) {
        address
        version
        module(name: "m") {
            functions { nodes { name } }
        }

        packageVersions {
            nodes {
                address
                version
            }
        }
    }

    packages(first: 10) {
        nodes {
            address
            version
        }
    }
}

//# upgrade --package P1 --upgrade-capability 1,1 --sender A
module P2::m {
    public fun f(): u64 { 42 }
    public fun g(): u64 { 43 }
    public fun h(): u64 { 44 }
}

//# create-checkpoint

//# run-graphql
{
    latestPackage(address: "@{P0}") {
        version
        module(name: "m") {
            functions { nodes { name } }
        }

        packageVersions {
            nodes {
                address
                version
            }
        }
    }

    firstPackage: package(address: "@{P2}", version: 1) {
        address
        version
        module(name: "m") {
            functions { nodes { name } }
        }

        packageVersions {
            nodes {
                address
                version
            }
        }
    }

    packages(first: 10) {
        nodes {
            address
            version
        }
    }
}

//# run-graphql
{   # Test fetching by ID
    v1: package(address: "@{P0}") {
        module(name: "m") {
            functions { nodes { name } }
        }

        latestPackage {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
    }

    v2: package(address: "@{P1}") {
        module(name: "m") {
            functions { nodes { name } }
        }

        latestPackage {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
    }

    v3: package(address: "@{P2}") {
        module(name: "m") {
            functions { nodes { name } }
        }

        latestPackage {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
    }
}

//# run-graphql
{   # Test fetching by version
    v1_from_p1: package(address: "@{P1}", version: 1) {
        module(name: "m") {
            functions { nodes { name } }
        }
    }

    v1_from_p2: package(address: "@{P2}", version: 1) {
        module(name: "m") {
            functions { nodes { name } }
        }
    }

    v2_from_p0: package(address: "@{P0}", version: 2) {
        module(name: "m") {
            functions { nodes { name } }
        }
    }

    v2_from_p2: package(address: "@{P2}", version: 2) {
        module(name: "m") {
            functions { nodes { name } }
        }
    }

    v3_from_p0: package(address: "@{P0}", version: 3) {
        module(name: "m") {
            functions { nodes { name } }
        }
    }

    v3_from_p1: package(address: "@{P1}", version: 3) {
        module(name: "m") {
            functions { nodes { name } }
        }
    }
}

//# run-graphql
{   # Go from one version to another using packageAtVersion
    v1: package(address: "@{P1}") {
        v1: packageAtVersion(version: 1) {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
        v2: packageAtVersion(version: 2) {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
        v3: packageAtVersion(version: 3) {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
    }

    v2: package(address: "@{P2}") {
        v1: packageAtVersion(version: 1) {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
        v2: packageAtVersion(version: 2) {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
        v3: packageAtVersion(version: 3) {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
    }

    v3: package(address: "@{P2}") {
        v1: packageAtVersion(version: 1) {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
        v2: packageAtVersion(version: 2) {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
        v3: packageAtVersion(version: 3) {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
    }
}

//# run-graphql
{   # Fetch out of range versions (should return null)
    v0: package(address: "@{P0}", version: 0) {
        module(name: "m") {
            functions { nodes { name } }
        }
    }

    # This won't return null, but its inner queries will
    v1: package(address: "@{P0}") {
        v0: packageAtVersion(version: 0) {
            module(name: "m") {
                functions { nodes { name } }
            }
        }

        v4: packageAtVersion(version: 4) {
            module(name: "m") {
                functions { nodes { name } }
            }
        }
    }

    v4: package(address: "@{P0}", version: 4) {
        module(name: "m") {
            functions { nodes { name } }
        }
    }
}

//# run-graphql
{   # Querying packages with checkpoint bounds
    before: packages(first: 10, filter: { beforeCheckpoint: 1 }) {
        nodes {
            address
            version
            previousTransactionBlock {
                effects { checkpoint { sequenceNumber } }
            }
        }
    }

    after: packages(first: 10, filter: { afterCheckpoint: 1 }) {
        nodes {
            address
            version
            previousTransactionBlock {
                effects { checkpoint { sequenceNumber } }
            }
        }
    }

    between: packages(first: 10, filter: { afterCheckpoint: 1, beforeCheckpoint: 3 }) {
        nodes {
            address
            version
            previousTransactionBlock {
                effects { checkpoint { sequenceNumber } }
            }
        }
    }
}

//# run-graphql
{   # Query for versions of a user package
    packageVersions(address: "@{P0}") {
        nodes {
            address
            version
        }
    }

    after: packageVersions(address: "@{P0}", filter: { afterVersion: 1 }) {
        nodes {
            address
            version
        }
    }

    before: packageVersions(address: "@{P0}", filter: { beforeVersion: 3 }) {
        nodes {
            address
            version
        }
    }

    between: packageVersions(
        address: "@{P0}",
        filter: {
          afterVersion: 1,
          beforeVersion: 3,
        },
    ) {
        nodes {
            address
            version
        }
    }
}

//# run-graphql
{   # Query for versions of a system package (there will be only one because we
    # don't have a way to upgrade system packages in these tests.)
    packageVersions(address: "0x1") {
        nodes {
            address
            version
        }
    }

    package(address: "0x1") {
        packageVersions {
            nodes {
                address
                version
            }
        }
    }
}
