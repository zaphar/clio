{
  inputs = {
    nixpkgs.url = "nixpkgs";
    rust-overlay = {
      url = "github:oxalica/rust-overlay?ref=stable";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    naersk.url = "github:nix-community/naersk";
    flake-utils = {url = "github:numtide/flake-utils"; flake=true; };
    flake-compat = { url = "github:edolstra/flake-compat"; flake = false; };
  };

  outputs = {nixpkgs, flake-utils, rust-overlay, naersk, ...}:
  flake-utils.lib.eachDefaultSystem (system: let
    overlays = [
      rust-overlay.overlays.default
    ];
    pkgs = import nixpkgs { inherit system overlays; };
    rust-bin = pkgs.rust-bin.stable."1.78.0".default;
    naersk-lib = pkgs.callPackage naersk {
        rustc = rust-bin;
        cargo = rust-bin;
    };
  in with pkgs;
  rec {
    clio = naersk-lib.buildPackage {
        pname = "clio";
        src = ./.;
        nativeBuildInputs = [pkgs.pkg-config];
        buildInputs =
          (
            if pkgs.stdenv.isDarwin
            then with pkgs.darwin.apple_sdk.frameworks; [Security SystemConfiguration]
            else [pkgs.openssl]
          )
          ++ [rust-bin];
    };
    packages.clio = clio;
    packages.default = clio;
    devShell = mkShell {
      buildInputs = (
        if stdenv.isDarwin then
          [ pkgs.darwin.apple_sdk.frameworks.Security pkg-config ]
        else
        [ ]) ++ [rust-bin];
    };
  });
}
