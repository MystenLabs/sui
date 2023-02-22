# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Copyright (c) The Diem Core Contributors
# Copyright (c) The Move Contributors
# SPDX-License-Identifier: Apache-2.0

# This script draws on the implementation of the dev_setup.sh script in the core Move repository
# installing all Move dependencies but restricts installed components to those required by the Move
# Prover.

# fast fail.
set -eo pipefail

Z3_VERSION=4.11.2
CVC5_VERSION=0.0.3
DOTNET_VERSION=6.0
BOOGIE_VERSION=2.15.8

SCRIPT_PATH="$(cd "$(dirname "$0")" >/dev/null 2>&1 && pwd)"
cd "$SCRIPT_PATH/.." || exit

function install_pkg {
  package=$1
  PACKAGE_MANAGER=$2
  PRE_COMMAND=()
  if [ "$(whoami)" != 'root' ]; then
    PRE_COMMAND=(sudo)
  fi
  if command -v "$package" &>/dev/null; then
    echo "$package is already installed"
  else
    echo "Installing ${package}."
    if [[ "$PACKAGE_MANAGER" == "yum" ]]; then
      "${PRE_COMMAND[@]}" yum install "${package}" -y
    elif [[ "$PACKAGE_MANAGER" == "apt-get" ]]; then
      "${PRE_COMMAND[@]}" apt-get install "${package}" --no-install-recommends -y
      echo apt-get install result code: $?
    elif [[ "$PACKAGE_MANAGER" == "pacman" ]]; then
      "${PRE_COMMAND[@]}" pacman -Syu "$package" --noconfirm
    elif [[ "$PACKAGE_MANAGER" == "apk" ]]; then
      apk --update add --no-cache "${package}"
    elif [[ "$PACKAGE_MANAGER" == "dnf" ]]; then
      dnf install "$package"
    elif [[ "$PACKAGE_MANAGER" == "brew" ]]; then
      brew install "$package"
    fi
  fi
}

function install_dotnet {
  echo "Installing .Net"
  mkdir -p "${DOTNET_INSTALL_DIR}dotnet/" || true
  if [[ $("${DOTNET_INSTALL_DIR}dotnet" --list-sdks | grep -c "^${DOTNET_VERSION}" || true) == "0" ]]; then
    if [[ "$(uname)" == "Linux" ]]; then
      # Install various prerequisites for .dotnet. There are known bugs
      # in the dotnet installer to warn even if they are present. We try
      # to install anyway based on the warnings the dotnet installer creates.
      if [ "$PACKAGE_MANAGER" == "apk" ]; then
        install_pkg icu "$PACKAGE_MANAGER"
        install_pkg zlib "$PACKAGE_MANAGER"
        install_pkg libintl "$PACKAGE_MANAGER"
        install_pkg libcurl "$PACKAGE_MANAGER"
      elif [ "$PACKAGE_MANAGER" == "apt-get" ]; then
        install_pkg gettext "$PACKAGE_MANAGER"
        install_pkg zlib1g "$PACKAGE_MANAGER"
      elif [ "$PACKAGE_MANAGER" == "yum" ] || [ "$PACKAGE_MANAGER" == "dnf" ]; then
        install_pkg icu "$PACKAGE_MANAGER"
        install_pkg zlib "$PACKAGE_MANAGER"
      elif [ "$PACKAGE_MANAGER" == "pacman" ]; then
        install_pkg icu "$PACKAGE_MANAGER"
        install_pkg zlib "$PACKAGE_MANAGER"
      fi
    fi
    # Below we need to (a) set TERM variable because the .net installer expects it and it is not set
    # in some environments (b) use bash not sh because the installer uses bash features.
    if [[ "$(uname)" == "Darwin" ]]; then
      # On Macs with M1 chip the dotnet-install.sh script will
      # attempt to download the Arm64 version which does not exist
      # for .NET 5.x so for now we have to force x64 version instead
      # to work for both architectures (in emulation mode for the M1
      # chip).
      curl -sSL https://dot.net/v1/dotnet-install.sh |
        TERM=linux /bin/bash -s -- --channel $DOTNET_VERSION --install-dir "${DOTNET_INSTALL_DIR}" --version latest --architecture "x64"
    else
      curl -sSL https://dot.net/v1/dotnet-install.sh |
        TERM=linux /bin/bash -s -- --channel $DOTNET_VERSION --install-dir "${DOTNET_INSTALL_DIR}" --version latest
    fi
  else
    echo Dotnet already installed.
  fi
}

function install_boogie {
  echo "Installing boogie"
  mkdir -p "${DOTNET_INSTALL_DIR}tools/" || true
  if [[ "$("${DOTNET_INSTALL_DIR}dotnet" tool list --tool-path "${DOTNET_INSTALL_DIR}tools/")" =~ .*boogie.*${BOOGIE_VERSION}.* ]]; then
    echo "Boogie $BOOGIE_VERSION already installed"
  else
    "${DOTNET_INSTALL_DIR}dotnet" tool update --tool-path "${DOTNET_INSTALL_DIR}tools/" Boogie --version $BOOGIE_VERSION
    # If a higher version of boogie is installed, we can not install required version with `dotnet tool update` command above
    # Print a tip here, since incompatible version of boogie might cause move-prover stuck forever
    if [[ $? != 0 ]]; then
      echo "failed to install boogie ${BOOGIE_VERSION}, if there is a more updated boogie installed, please consider uninstall it with"
      echo "${DOTNET_INSTALL_DIR}dotnet tool uninstall --tool-path ${DOTNET_INSTALL_DIR}/tools Boogie"
    fi
  fi
}

function install_z3 {
  echo "Installing Z3"
  mkdir -p "${INSTALL_DIR}" || true
  if command -v /usr/local/bin/z3 &>/dev/null; then
    echo "z3 already exists at /usr/local/bin/z3"
    echo "but this install will go to ${INSTALL_DIR}/z3."
    echo "you may want to remove the shared instance to avoid version confusion"
  fi
  if command -v "${INSTALL_DIR}z3" &>/dev/null && [[ "$("${INSTALL_DIR}z3" --version || true)" =~ .*${Z3_VERSION}.* ]]; then
    echo "Z3 ${Z3_VERSION} already installed"
    return
  fi
  if [[ "$(uname)" == "Linux" ]]; then
    Z3_PKG="z3-$Z3_VERSION-x64-glibc-2.31"
  elif [[ "$(uname)" == "Darwin" ]]; then
    Z3_PKG="z3-$Z3_VERSION-x64-osx-10.16"
  else
    echo "Z3 support not configured for this platform (uname=$(uname))"
    return
  fi
  TMPFILE=$(mktemp)
  rm "$TMPFILE"
  mkdir -p "$TMPFILE"/
  (
    cd "$TMPFILE" || exit
    curl -LOs "https://github.com/Z3Prover/z3/releases/download/z3-$Z3_VERSION/$Z3_PKG.zip"
    unzip -q "$Z3_PKG.zip"
    cp "$Z3_PKG/bin/z3" "${INSTALL_DIR}"
    chmod +x "${INSTALL_DIR}z3"
  )
  rm -rf "$TMPFILE"
}

function install_cvc5 {
  echo "Installing cvc5"
  mkdir -p "${INSTALL_DIR}" || true
  if command -v /usr/local/bin/cvc5 &>/dev/null; then
    echo "cvc5 already exists at /usr/local/bin/cvc5"
    echo "but this install will go to $${INSTALL_DIR}cvc5."
    echo "you may want to remove the shared instance to avoid version confusion"
  fi
  if command -v "${INSTALL_DIR}cvc5" &>/dev/null && [[ "$("${INSTALL_DIR}cvc5" --version || true)" =~ .*${CVC5_VERSION}.* ]]; then
    echo "cvc5 ${CVC5_VERSION} already installed"
    return
  fi
  if [[ "$(uname)" == "Linux" ]]; then
    CVC5_PKG="cvc5-Linux"
  elif [[ "$(uname)" == "Darwin" ]]; then
    CVC5_PKG="cvc5-macOS"
  else
    echo "cvc5 support not configured for this platform (uname=$(uname))"
    return
  fi
  TMPFILE=$(mktemp)
  rm "$TMPFILE"
  mkdir -p "$TMPFILE"/
  (
    cd "$TMPFILE" || exit
    curl -LOs "https://github.com/cvc5/cvc5/releases/download/cvc5-$CVC5_VERSION/$CVC5_PKG"
    cp "$CVC5_PKG" "${INSTALL_DIR}cvc5"
    chmod +x "${INSTALL_DIR}cvc5"
  )
  rm -rf "$TMPFILE"
}

if [[ "$VERBOSE" == "true" ]]; then
  set -x
fi

if [[ "$(uname)" == "Linux" ]]; then
  # check for default package manager for linux
  if [[ -f /etc/redhat-release ]]; then
    # use yum by default
    if command -v yum &>/dev/null; then
      PACKAGE_MANAGER="yum"
    elif command -v dnf &>/dev/null; then
      # dnf is the updated default since Red Hat Enterprise Linux 8, CentOS 8, Fedora 22, and any distros based on these
      echo "WARNING: dnf package manager support is experimental"
      PACKAGE_MANAGER="dnf"
    fi
  elif [[ -f /etc/debian_version ]] && command -v apt-get &>/dev/null; then
    PACKAGE_MANAGER="apt-get"
  elif [[ -f /etc/arch-release ]] && command -v pacman &>/dev/null; then
    PACKAGE_MANAGER="pacman"
  elif [[ -f /etc/alpine-release ]] && command -v apk &>/dev/null; then
    PACKAGE_MANAGER="apk"
  fi
  # if no default PACKAGE_MANAGER detected, pick one that's installed, this is usually useless
  if [[ $PACKAGE_MANAGER == "" ]]; then
    if command -v yum &>/dev/null; then
      PACKAGE_MANAGER="yum"
    elif command -v apt-get &>/dev/null; then
      PACKAGE_MANAGER="apt-get"
    elif command -v pacman &>/dev/null; then
      PACKAGE_MANAGER="pacman"
    elif command -v apk &>/dev/null; then
      PACKAGE_MANAGER="apk"
    elif command -v dnf &>/dev/null; then
      echo "WARNING: dnf package manager support is experimental"
      PACKAGE_MANAGER="dnf"
    else
      echo "Unable to find supported package manager (yum, apt-get, dnf, or pacman). Abort"
      exit 1
    fi
  fi
elif [[ "$(uname)" == "Darwin" ]]; then
  if command -v brew &>/dev/null; then
    PACKAGE_MANAGER="brew"
  else
    echo "Missing package manager Homebrew (https://brew.sh/). Abort"
    exit 1
  fi
else
  echo "Unknown OS. Abort."
  exit 1
fi

if [[ "$PACKAGE_MANAGER" == "apt-get" ]]; then
  install_pkg ca-certificates "$PACKAGE_MANAGER"
fi

export DOTNET_INSTALL_DIR="${HOME}/.dotnet/"
export INSTALL_DIR="$HOME/bin/"
install_pkg curl "$PACKAGE_MANAGER"
install_pkg unzip "$PACKAGE_MANAGER"
install_z3
install_cvc5
install_dotnet
install_boogie

exit 0
