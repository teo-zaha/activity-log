# Build a Windows 11 (ARM) Vagrant box for VMware Fusion on Apple Silicon.
#
# There is no public turnkey Windows-on-ARM Vagrant box, so we build one once
# from a Microsoft Windows 11 ARM ISO. The result is added as the local box
# `activity-log/windows-11-arm`, which infra/windows-vm/Vagrantfile consumes.
#
# Run via:  pnpm win:box:build   (cd infra/windows-vm/packer && packer build windows11-arm.pkr.hcl)
#
# You must supply the ISO yourself (licensing); set iso_path / iso_checksum
# below or via -var. The autounattend.xml + scripts/ perform an unattended
# install, enable OpenSSH + WinRM, and seed the Vagrant insecure key.

packer {
  required_plugins {
    vmware = {
      version = ">= 1.1.0"
      source  = "github.com/hashicorp/vmware"
    }
  }
}

variable "iso_path" {
  type        = string
  description = "Path to the Windows 11 ARM64 ISO (supply your own)."
  default     = "iso/Win11_ARM64.iso"
}

variable "iso_checksum" {
  type        = string
  description = "Checksum of the ISO, e.g. 'sha256:...'."
  default     = "none"
}

variable "winrm_password" {
  type    = string
  default = "vagrant"
}

source "vmware-iso" "windows11arm" {
  display_name     = "activity-log-windows-11-arm"
  guest_os_type    = "arm-windows11-64"
  iso_url          = var.iso_path
  iso_checksum     = var.iso_checksum
  version          = 20

  cpus             = 4
  memory           = 8192
  disk_size        = 61440
  disk_adapter_type = "nvme"
  network_adapter_type = "e1000e"

  headless         = true

  # Unattended install: the answer file is served on a virtual floppy/CD.
  floppy_files = [
    "autounattend.xml",
    "scripts/enable-ssh.ps1",
    "scripts/enable-winrm.ps1",
  ]

  # Vagrant drives the box over SSH; provisioning bootstraps via WinRM first.
  communicator   = "winrm"
  winrm_username = "vagrant"
  winrm_password = var.winrm_password
  winrm_timeout  = "2h"

  shutdown_command = "shutdown /s /t 10 /f /d p:4:1 /c \"Packer Shutdown\""
  shutdown_timeout = "30m"
}

build {
  sources = ["source.vmware-iso.windows11arm"]

  provisioner "powershell" {
    scripts = [
      "scripts/enable-ssh.ps1",
      "scripts/install-vmware-tools.ps1",
    ]
  }

  post-processor "vagrant" {
    provider_override = "vmware_desktop"
    output            = "activity-log-windows-11-arm.box"
  }

  # After the build, add the box locally:
  #   vagrant box add activity-log/windows-11-arm activity-log-windows-11-arm.box --force
}
