# Enable the Windows OpenSSH Server and seed the Vagrant insecure public key so
# `vagrant ssh` works from the Mac host. Default the SSH shell to PowerShell.

$ErrorActionPreference = "Stop"

Write-Host "Installing OpenSSH Server..."
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0

Set-Service -Name sshd -StartupType Automatic
Start-Service sshd

# Use PowerShell as the default shell for SSH sessions.
New-ItemProperty -Path "HKLM:\SOFTWARE\OpenSSH" -Name DefaultShell `
  -Value "C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" `
  -PropertyType String -Force | Out-Null

# Install the Vagrant insecure public key (well-known, fetched from the
# canonical source so we never commit a stale/wrong key). Paired with
# `config.ssh.insert_key = false`, this lets `vagrant ssh` authenticate.
$insecureKey = (Invoke-WebRequest -UseBasicParsing `
  -Uri "https://raw.githubusercontent.com/hashicorp/vagrant/main/keys/vagrant.pub").Content.Trim()
$sshDir = "C:\ProgramData\ssh"
New-Item -ItemType Directory -Force -Path $sshDir | Out-Null
# administrators_authorized_keys is used for accounts in the Administrators group.
Set-Content -Path "$sshDir\administrators_authorized_keys" -Value $insecureKey -Encoding ascii
icacls "$sshDir\administrators_authorized_keys" /inheritance:r /grant "Administrators:F" /grant "SYSTEM:F" | Out-Null

# Firewall rule for SSH.
if (-not (Get-NetFirewallRule -Name "sshd" -ErrorAction SilentlyContinue)) {
  New-NetFirewallRule -Name sshd -DisplayName "OpenSSH Server (sshd)" `
    -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22
}

Write-Host "OpenSSH Server enabled."
