#!/bin/bash
# Verifica OtterSwiss desde WSL con target Windows (cargo check / clippy).
#
# Alternativa a cargo en Windows. Hacía falta cuando Smart App Control bloqueaba los ejecutables sin firma que genera
# cargo (`os error 4551`); el autor lo apagó el 24/11/2026 y cargo corre en Windows. Desde WSL se
# compila contra x86_64-pc-windows-msvc sin ejecutar nada en Windows: verifica tipos, borrow checker y
# clippy, pero NO corre tests ni enlaza el .exe.
#
# Uso (desde PowerShell):
#   wsl.exe -d Ubuntu -- bash "/mnt/d/Documentos/Progra/Proyectos Personales/OtterSwiss/.agents/scripts/check-wsl.sh" check
#   wsl.exe -d Ubuntu -- bash ".../check-wsl.sh" clippy -- -D warnings
#   wsl.exe -d Ubuntu -- bash ".../check-wsl.sh" clippy --all-targets -- -D warnings   # incluye los tests
#
# `test --no-run` no sirve aquí: enlazar un binario MSVC exige `link.exe`, que WSL no tiene. Para comprobar que
# los tests compilan se usa `clippy --all-targets`. `cargo fmt` sí corre en Windows (Smart App Control no lo bloquea; comprobado el 2026-11-24).
#
# Requisitos en Ubuntu (WSL): rustup con target x86_64-pc-windows-msvc y build-essential.
#   curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal -c clippy
#   rustup target add x86_64-pc-windows-msvc
#   apt-get install -y build-essential

source "$HOME/.cargo/env"
cd "$(dirname "$0")/../.." || exit 1
export CARGO_TARGET_DIR="$HOME/otter-target"
cmd="${1:-check}"
shift || true
cargo "$cmd" --target x86_64-pc-windows-msvc "$@" 2>&1
