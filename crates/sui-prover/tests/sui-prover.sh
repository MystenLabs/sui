#!/bin/bash

set -e  # Exit on errors
set -o pipefail  # Exit on pipe failures
set -x  # Enable debugging (optional)

# Define installation directories in the user's home directory (writable in GitHub Actions)
INSTALL_DIR="$HOME/.local/libexec/sui-prover"
BIN_DIR="$HOME/.local/bin"

# Create necessary directories
mkdir -p "$INSTALL_DIR" "$BIN_DIR"

# Install dependencies using Homebrew
brew install dotnet@8 rust z3

# Set environment variables
export DOTNET_ROOT="$(brew --prefix dotnet@8)/libexec"
export PATH="$DOTNET_ROOT/bin:$BIN_DIR:$PATH"

# Clone and build Sui Prover
cargo install --locked --path ./crates/sui-prover --root "$INSTALL_DIR"

# Clone and build Boogie
git clone --branch master https://github.com/boogie-org/boogie.git boogie-src
cd boogie-src
dotnet build Source/Boogie.sln -c Release
cp -r Source/BoogieDriver/bin/Release/net8.0/* "$INSTALL_DIR"
ln -s "$INSTALL_DIR/BoogieDriver" "$BIN_DIR/boogie"
cd ..
rm -rf boogie-src

# Create sui-prover wrapper script
cat <<EOF > "$BIN_DIR/sui-prover"
#!/bin/bash
export DOTNET_ROOT="$DOTNET_ROOT"
export BOOGIE_EXE="$BIN_DIR/boogie"
export Z3_EXE="$(brew --prefix z3)/bin/z3"
exec "$INSTALL_DIR/bin/sui-prover" "\$@"
EOF

chmod +x "$BIN_DIR/sui-prover"

# Add $BIN_DIR to PATH in the current session
echo "export PATH=$BIN_DIR:\$PATH" >> ~/.bashrc
echo "Sui Prover installed successfully! Use 'sui-prover' command."

# Run cargo test in sui-prover directory
export DOTNET_ROOT="$DOTNET_ROOT"
export BOOGIE_EXE="$BIN_DIR/boogie"
export Z3_EXE="$(brew --prefix z3)/bin/z3"
cd ./crates/sui-prover
cargo test
cd ../..


cd ./crates/sui-framework/packages/prover
sui-prover
cd ..

git clone https://github.com/andrii-a8c/sui-kit.git sui-kit
cd sui-kit/examples

cd amm
sui-prover
cd .. 

cd guide
sui-prover
cd ../../..

git clone https://github.com/asymptotic-code/sui-prover-workshop.git prover-workshop
cd prover-workshop

sui-prover --split-paths=4

echo "All tests passed!"
