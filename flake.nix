{
  description = "Wondering";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/master";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;

          overlays = [ rust-overlay.overlays.default ];
        };
      in
      {
        formatter = pkgs.nixpkgs-fmt;
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rust-bin.nightly.latest.default
            armips
          ];

          PSX_BIOS = pkgs.fetchurl {
            url = "https://github.com/Abdess/retrobios/raw/refs/heads/main/bios/Sony/PlayStation/openbios.bin";
            hash = "sha256-YjV8epvYjNzL64Mb0gxLHxhVHjwTrPRm1ObWNkNLNIs=";
          };
        };
      }
    );
}
