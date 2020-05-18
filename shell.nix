# shell.nix: Dependency management with the nix package manager.
# Author: Håkon Jordet
# Copyright (c) 2020 LAPS Group
# Distributed under the zlib licence, see LICENCE.

{ pkgs ? import <nixpkgs> {} }:
pkgs.stdenv.mkDerivation {
  name = "laps-shell";
  src = null;
  buildInputs = [pkgs.gdal_2 pkgs.llvmPackages.libclang pkgs.clang pkgs.nodejs];
  shellHook = ''
export LIBCLANG_PATH=${pkgs.llvmPackages.libclang}/lib
export GDAL_INCLUDE_DIR=${pkgs.gdal_2}/include
export GDAL_LIB_DIR=${pkgs.gdal_2}/lib
'';
}
