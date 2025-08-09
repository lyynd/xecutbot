#!/usr/bin/env bash
set -euxo pipefail

# The system of the target host
SYSTEM=${SYSTEM:-x86_64-linux}

# The desired package
FLAKE_PATH=".#packages.${SYSTEM}.default"

echo "Building"

nix build $FLAKE_PATH

# Get the Nix store path
NIX_STORE_PATH="$(nix path-info $FLAKE_PATH)"

PNAME="$(nix eval --raw $FLAKE_PATH.pname)"

TARGET="${1}"

echo "Starting deployment"

run="ssh ${TARGET} --"

echo "Copying ${FLAKE_PATH} to ${TARGET}"
nix copy --no-check-sigs --to "ssh-ng://${TARGET}" $FLAKE_PATH

echo "Removing $PNAME"

$run nix profile remove "$PNAME"

echo "Installing ${FLAKE_PATH} to profile"
$run nix profile install "${NIX_STORE_PATH}"
