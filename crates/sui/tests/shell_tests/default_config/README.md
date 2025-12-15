Tests in this directory are around the behavior for creating an empty client
config (i.e. when ~/.sui doesn't exist)

These tests all use an explicit --client.config argument so that they don't
interfere with the dev's config or each other.
