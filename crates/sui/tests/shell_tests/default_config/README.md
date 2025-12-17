Tests in this directory are around the behavior for creating an empty client
config (i.e. when ~/.sui doesn't exist)

These tests all use an explicit --client.config argument so that they don't
interfere with the dev's config or each other.

We use `sui move new` because all of the `client` commands immediately contact
the network and print a warning and we'd like to avoid that in CI.
