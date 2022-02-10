#!/bin/sh
# Run this if you just want a fresh CLI instance to play with

# Build
echo
echo "-------------------------"
echo "Building FastX"
echo "-------------------------"
echo
cargo build --release

# Kill old instances if any
echo
echo "-------------------------"
echo "Killing old instances"
echo "-------------------------"
echo
killall fastx
killall wallet

# Nav to binaries
echo
echo "-------------------------"
echo "Finding binaries"
echo "-------------------------"
echo
if [ -d "../target/release" ] 
then
    cd ../target/release 
elif [ -d "target/release" ] 
then
    cd target/release 
else
    echo "Could not find binaries"
    echo "Run script at project root"
    exit 1
fi

# Delete old configs and states if any
echo
echo "-------------------------"
echo "Removing old configs"
echo "-------------------------"
echo
rm network.conf
rm -rf client_db
rm *.kf

# Init
echo
echo "-------------------------"
echo "Creating genesis state"
echo "-------------------------"
echo
./fastx genesis

# Start authorities in bg
echo
echo "-------------------------"
echo "Starting authorities...."
echo "-------------------------"
echo
./fastx start &

# Wait for all to start
sleep 4

# Start interactive CLI
echo
echo "-------------------------"
echo "Starting wallet CLI...."
echo "-------------------------"
echo
./wallet