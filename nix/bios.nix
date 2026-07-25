{ ... }:
{
  perSystem =
    { pkgs, ... }:
    {
      packages.bios = pkgs.fetchurl {
        url = "https://github.com/Abdess/retrobios/raw/refs/heads/main/bios/Sony/PlayStation/scph1001.bin";
        hash = "sha256-ca+U0eR6aMEej9ufg2gEBgFRSkKlo5nNpIx9O/8emdM=";
      };
    };
}
