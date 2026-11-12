# Configure WinRM so Packer (and Vagrant, as a fallback) can connect during the
# unattended install. Invoked from autounattend.xml's FirstLogonCommands.

$ErrorActionPreference = "Stop"

# Bring the network up as Private so WinRM/firewall rules apply.
Get-NetConnectionProfile | ForEach-Object {
  Set-NetConnectionProfile -InterfaceIndex $_.InterfaceIndex -NetworkCategory Private -ErrorAction SilentlyContinue
}

winrm quickconfig -quiet
winrm set winrm/config/service '@{AllowUnencrypted="true"}'
winrm set winrm/config/service/auth '@{Basic="true"}'
winrm set winrm/config/winrs '@{MaxMemoryPerShellMB="1024"}'

Set-Service -Name WinRM -StartupType Automatic
Start-Service -Name WinRM

if (-not (Get-NetFirewallRule -Name "winrm-http" -ErrorAction SilentlyContinue)) {
  New-NetFirewallRule -Name winrm-http -DisplayName "WinRM HTTP" `
    -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 5985
}

Write-Host "WinRM configured."
