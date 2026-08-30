# Native package manager recipes

This directory contains the Conan 2 recipe and vcpkg overlay port for the
Ironpress C and C++ bindings. Both packages expose the same stable C ABI and
header-only C++17 wrapper; they do not introduce another renderer or C++ ABI.

The recipes are validated in this repository before they are proposed to
ConanCenter and the curated vcpkg registry. Until those external submissions
are accepted, use these recipes from an Ironpress checkout. The public package
names must not be advertised as generally available before that happens.

## Why both recipes build from source

ConanCenter requires recipes to build from source rather than repackage
precompiled binaries. The vcpkg port follows the same model so both registries
exercise one contract. Each recipe fetches an immutable Ironpress tag, injects
the pinned workspace lockfile, and verifies both checksums before building with
`--locked`.

Building requires Rust 1.88 or newer on the target host. The initial recipes
support native builds for Linux and macOS on x86-64 and ARM64, and Windows on
x86-64. Cross-compilation is rejected instead of silently producing an artifact
for an untested contract. Consumers do not need Rust after packaging.

The source policy and port structure follow the upstream guidance:

- [ConanCenter sources and patches](https://github.com/conan-io/conan-center-index/blob/master/docs/adding_packages/sources_and_patches.md)
- [ConanCenter recipe structure](https://github.com/conan-io/conan-center-index/blob/master/docs/adding_packages/folders_and_files.md)
- [vcpkg `vcpkg_from_github`](https://learn.microsoft.com/vcpkg/maintainers/functions/vcpkg_from_github)
- [vcpkg maintainer guide](https://learn.microsoft.com/vcpkg/contributing/maintainer-guide)

## Validate Conan

Install Conan 2, detect the native profile, then create both linkage variants:

```bash
conan profile detect --force
conan create packaging/conan/all --version=1.6.0 --build=missing
conan create packaging/conan/all --version=1.6.0 --build=missing \
  -o 'ironpress/*:shared=True'
```

The canonical `test_package` installs each created package through CMake, then
renders a PDF from independent C and C++ consumers.

After creating the package locally, a project can resolve it with:

```bash
conan install --requires=ironpress/1.6.0 --build=missing
```

## Validate vcpkg

Bootstrap a vcpkg checkout and pass its path plus a native triplet to the test
driver:

```bash
packaging/vcpkg/test/run.sh /path/to/vcpkg x64-linux
packaging/vcpkg/test/run.sh /path/to/vcpkg x64-linux-dynamic
```

The Windows workflow uses `run.ps1` with the same two arguments. A project can
consume the local overlay directly:

```bash
vcpkg install ironpress --overlay-ports=packaging/vcpkg/ports
```

Both managers export `Ironpress::C` and `Ironpress::CXX`. Package tests compile
and run both consumers for static and shared linkage on every advertised host.

## Update after a release

Package-manager metadata is updated only after the release tag exists, because
the immutable source archive and its checksum are part of each recipe.

1. Update the version in `packaging/conan/config.yml`, `conandata.yml`, and the
   vcpkg manifest.
2. Pin the released `Cargo.lock` URL and its Conan SHA-256 and vcpkg SHA-512.
3. Update the Conan SHA-256 and vcpkg SHA-512 from the tagged source archive.
4. Run the Conan and vcpkg static and shared consumer tests.
5. Let the full native CI matrix pass before opening external registry updates.

The `v1.6.0` source tag predates the committed workspace lockfile, so its recipe
uses the immutable lockfile from the corresponding release merge. Future
release tags include `Cargo.lock`, which remains pinned separately so an absent
or mismatched lock cannot silently produce an unlocked package.
