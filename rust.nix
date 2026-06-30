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
        (rust-bin.nightly.latest.default.override {
          targets = [ "wasm32-unknown-unknown" ];
        })
        cargo-show-asm
        samply
        trunk
        wasm-bindgen-cli
      ];
    };
  };
}
