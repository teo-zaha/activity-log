# Install the Rust toolchain and the native ARM64 MSVC build prerequisites.
# Runs as a Vagrant provisioner ("bootstrap") and is safe to re-run.
#
# This is the heavy one-time step: it pulls rustup and the VS 2022 Build Tools
# (ARM64 components) so `cargo` can build the native windows-msvc target that
# matches production.

$ErrorActionPreference = "Stop"

function Test-Command($name) {
  return [bool](Get-Command $name -ErrorAction SilentlyContinue)
}

# Visual Studio 2022 Build Tools (ARM64 toolchain + Windows SDK) via winget.
if (-not (Test-Path "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools")) {
  Write-Host "Installing Visual Studio 2022 Build Tools (ARM64)..."
  winget install --id Microsoft.VisualStudio.2022.BuildTools `
    --accept-package-agreements --accept-source-agreements `
    --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --add Microsoft.VisualStudio.Component.Windows11SDK.22621"
}

# rustup + default stable toolchain (host is aarch64-pc-windows-msvc on ARM).
if (-not (Test-Command "rustup")) {
  Write-Host "Installing rustup..."
  $rustup = "$env:TEMP\rustup-init.exe"
  Invoke-WebRequest -Uri "https://win.rustup.rs/aarch64" -OutFile $rustup
  & $rustup -y --default-toolchain stable --profile minimal
  $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
}

& "$env:USERPROFILE\.cargo\bin\rustup.exe" component add clippy rustfmt
& "$env:USERPROFILE\.cargo\bin\rustc.exe" --version
Write-Host "Rust toolchain ready."
