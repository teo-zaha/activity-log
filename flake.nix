{
  description = "Development environment";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    # Tools
    flake-parts.url = "github:hercules-ci/flake-parts";
    flake-root.url = "github:srid/flake-root";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # process-compose-flake.url = "github:Platonic-Systems/process-compose-flake";
  };

  outputs =
    inputs@{ flake-parts, nixpkgs, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "aarch64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];

      imports = [
        inputs.flake-root.flakeModule
        # inputs.process-compose-flake.flakeModule
      ];

      perSystem =
        {
          self',
          pkgs,
          system,
          lib,
          config,
          ...
        }:
        let
          bun = pkgs.bun;
        in
        {
          _module.args.pkgs = import nixpkgs {
            inherit system;
            config.allowUnfree = true;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };
          devShells.default = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              bun
              cargo
              clippy
              nodejs
              pnpm_10
              python3
              rustc
              rustfmt
            ];

            packages = with pkgs; [
              git-cliff
              just
            ];

            inputsFrom = [
              config.flake-root.devShell
            ];
          };

          # Cross-builds the Tauri Windows installer (x86_64-pc-windows-msvc) via
          # cargo-xwin + NSIS. Used by `pnpm desktop:build:windows`.
          devShells.windows = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              (rust-bin.stable.latest.default.override {
                targets = [ "x86_64-pc-windows-msvc" ];
              })
              cargo-xwin
              llvmPackages.clang-unwrapped
              llvmPackages.llvm
              llvmPackages.lld
              nsis
              nodejs
              pnpm_10
            ];
          };
        };
    };
}
