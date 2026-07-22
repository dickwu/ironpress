# Historical parity generators

Each `sha256/<digest>/parity-gen-refs.sh` is an immutable, byte-exact copy of
the generator recorded by oracle provenance. Validation accepts an archived
generator only when both the directory name and the file content match that
digest. Never edit an archived file; add a new digest directory instead.
