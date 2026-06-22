{ inputs, ... }: {
  perSystem = { pkgs, system, ... }: {
    _module.args.pkgs = import inputs.nixpkgs {
      inherit system;

      overlays = [
        inputs.rust-overlay.overlays.default
      ];
    };

    devShells.default = pkgs.mkShell {
      buildInputs = with pkgs; [
        rust-bin.nightly.latest.default
        cargo-show-asm
        samply
      ];
    };
  };
}
