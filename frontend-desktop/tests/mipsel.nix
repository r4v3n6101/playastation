{ ... }:
{
  perSystem =
    { pkgs, ... }:
    {
      _module.args = {
        mipselPkgs = import pkgs.path {
          inherit (pkgs.stdenv.hostPlatform) system;

          crossSystem = {
            config = "mipsel-none-elf";
            gcc = {
              arch = "mips1";
              tune = "r3000";
            };
          };

          overlays = [
            (final: prev: {
              newlib = prev.newlib.overrideAttrs (old: {
                configureFlags = (old.configureFlags or [ ]) ++ [
                  "--disable-libgloss"
                ];
              });
            })
          ];
        };
      };
    };
}
