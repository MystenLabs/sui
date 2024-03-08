#!/bin/bash
# assumes sui cli installed (brew install sui)

DIR="genesis/files"

if [ -d "$DIR" ]; then
    echo "Directory $DIR exists. Removing..."
    rm -r "$DIR"
fi

echo "Creating directory $DIR..."
mkdir "$DIR"
echo "$DIR directory created."

pip3 install -r genesis/requirements.txt
genesis/generate.py --genesis-template genesis/compose-validators.yaml --target-directory "$DIR"
