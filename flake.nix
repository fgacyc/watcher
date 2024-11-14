{
  description = "Nix environment for development.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";
    flake-parts.url = "github:hercules-ci/flake-parts";
    flake-root.url = "github:srid/flake-root";
  };

  outputs = inputs@{ nixpkgs, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; }
      {
        imports = [
          inputs.flake-root.flakeModule
        ];
        systems = [
          "x86_64-linux"
          "aarch64-linux"
          "aarch64-darwin"
        ];
        perSystem = { pkgs, lib, config, ... }: {
          devShells.default = pkgs.mkShell rec {
            inputsFrom = [ config.flake-root.devShell ];

            name = "watcher";

            # dev tools
            nativeBuildInputs = with pkgs; [
              pkg-config # packages finder
              poetry # python env
              pyright # python lsp
              nodejs # node for python lsp

              rustup # for rust
              sccache # build caching
              bacon # background flycheck
            ];

            # libraries
            buildInputs = with pkgs; [ openssl zlib ];

            shellHook = ''
              export LD_LIBRARY_PATH=${pkgs.stdenv.cc.cc.lib}/lib/
              export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath buildInputs}:$LD_LIBRARY_PATH"
              export RUSTC_WRAPPER=$(which sccache)
            '';
          };
        };
      };
}

