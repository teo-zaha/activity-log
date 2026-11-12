# Install VMware Tools so the guest reports its IP to the host (needed for
# Vagrant SSH) and shuts down cleanly. On a Windows-on-ARM guest, mount the
# VMware Tools ARM64 installer (Packer attaches it as a CD) and run it silently.
#
# If your VMware Fusion version auto-installs Tools, this script is a no-op
# guard. Adjust the installer path to match the mounted Tools ISO.

$ErrorActionPreference = "Stop"

$setup = Get-ChildItem -Path "D:\", "E:\" -Filter "setup64.exe" -ErrorAction SilentlyContinue |
  Select-Object -First 1

if ($setup) {
  Write-Host "Installing VMware Tools from $($setup.FullName)..."
  Start-Process -FilePath $setup.FullName `
    -ArgumentList '/S /v "/qn REBOOT=R"' -Wait
} else {
  Write-Host "VMware Tools installer not found on D:/E: - assuming Tools are managed by Fusion."
}
