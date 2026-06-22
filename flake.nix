{
  description = "Wondering";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/master";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    import-tree = {
      url = "github:vic/import-tree";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";

    bios = {
      url = "https://github.com/Abdess/retrobios/raw/refs/heads/main/bios/Sony/PlayStation/scph1001.bin";
      flake = false;
    };

    peter-lemon-test-roms = {
      url = "github:PeterLemon/PSX";
      flake = false;
    };

    amidog-cpu-test-rom = {
      url = "tarball+https://psx.amidog.se/lib/exe/fetch.php?media=psx:download:psxtest_cpu.zip";
      flake = false;
    };

    amidog-cpx-test-rom = {
      url = "tarball+https://psx.amidog.se/lib/exe/fetch.php?media=psx:download:psxtest_cpx.zip";
      flake = false;
    };

    pcsx-redux = {
      url = "git+https://github.com/nicolasnoble/pcsx-redux?submodules=1";
      flake = false;
    };
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      imports = [
        (inputs.import-tree [
          ./rust.nix
          ./rom-tests
        ])
      ];
    };
}
