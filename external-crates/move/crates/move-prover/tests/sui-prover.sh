#!/bin/bash

set -e  # Exit immediately if a command fails
set -o pipefail  # Exit on errors in pipes
set -x  # Enable debugging (optional)

# Define installation directories
INSTALL_DIR="/usr/local/libexec/sui-prover"
BIN_DIR="/usr/local/bin"

# Install dependencies via Homebrew
brew install dotnet@8 rust z3

# Set environment variables
export DOTNET_ROOT="$(brew --prefix dotnet@8)/libexec"
export PATH="$DOTNET_ROOT/bin:$PATH"

# Clone and build Sui Prover
cargo install --locked --path ./crates/sui-prover
mkdir -p "$INSTALL_DIR"
mv target/release/sui-prover "$INSTALL_DIR"

# Clone and build Boogie
git clone --branch master https://github.com/boogie-org/boogie.git boogie-src
cd boogie-src
dotnet build Source/Boogie.sln -c Release
mkdir -p "$INSTALL_DIR"
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
exec "$INSTALL_DIR/sui-prover" "\$@"
EOF

chmod +x "$BIN_DIR/sui-prover"

# Success message
echo "Sui Prover installed successfully!"
