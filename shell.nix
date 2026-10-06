{ ... }: {
  perSystem =
    {
      pkgs,
      craneLib,
      ...
    }:
    {
      devShells.default = craneLib.devShell {
        packages = with pkgs; [
          cargo-show-asm
          samply
          wasm-bindgen-cli
          trunk
        ];
      };
    };
}
