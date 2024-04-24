{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, flake-utils, nixpkgs, rust-overlay }:

  flake-utils.lib.eachDefaultSystem (system:
    let

      overlays = [( import rust-overlay )];
      pkgs = (import nixpkgs) {
        inherit system overlays;
      };

      rust-version = "1.75.0";
      rust-toolchain = pkgs.rust-bin.stable.${rust-version}.default;

    in {
      # For `nix develop`:
      devShell = pkgs.mkShell {
        nativeBuildInputs = [
          rust-toolchain
          pkgs.cargo-release
          pkgs.protobuf
          pkgs.llvm
          # pkgs.gmp
          # pkgs.openssl
          # pkgs.libusb
          # pkgs.pkg-config
          # pkgs.libiconv
        ]
        ++ (pkgs.lib.optionals pkgs.stdenv.isDarwin [
          pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          # pkgs.darwin.apple_sdk.frameworks.AppKit
          # pkgs.darwin.apple_sdk.frameworks.WebKit
        ]);
      };
    }
  );
}