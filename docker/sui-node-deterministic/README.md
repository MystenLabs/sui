# Sui Node Deterministic Build

## General Requirements
* Requires Docker `>=v26.0.1`
* OCI-Compliant buildx `docker-container`: 
    * `docker buildx create --driver 'docker-container' --name stagex --use`
    * `docker use --bootstrap stagex`

## MacOS Requirements
* ensure previous requirements, `Builders` should look like:
![alt text](./images/image-2.png)

* in `General`, Enable `containerd for pulling and storing images`
![Docker Engine General Settings](./images/image.png)

* disable Rosetta
![alt text](./images/image-1.png)

## Build Steps
In Root Directory, run: `./docker/sui-node-deterministic/build.sh`

Build artifact is output in: `build/oci/sui-node`

Load the image with the command: `(cd build/oci/sui-node && tar -c .) | docker load`

## Extract sui-node Binary

### Find sui-node binary

Find oci blob with sui-node binary (it is the largest blob in `build/oci/sui-node/blobs/sha256`)
`ls -lSh build/oci/sui-node/blobs/sha256`

### Extract sui-node Binary

Extract `sui-node` binary from blob:
`tar xf build/oci/sui-node/blobs/sha256/<blob-digest>`

### Get digest of sui-node.

On Linux run:
`sha256sum opt/sui/bin/sui-node`

On MacOS run:
`shasum -a 256 opt/sui/bin/sui-node`