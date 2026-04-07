set shell := ["nu", "-c"]

build-git:
    cargo build --release --features=git

build:
    cargo build --release

shell:
    nix-shell -p protoc-gen-prost protobuf_33 --command nu

protoc:
    protoc --prost_out=src/protos -I src src/protos/backend.proto
    protoc --prost_out=src/protos -I src src/protos/op_store.proto
    protoc --prost_out=src/protos -I src src/protos/smart.proto

rm:
    rm -rf test*

init-git: rm
    jj git init --no-colocate test1
    jj git init --no-colocate test2
    jj git init --no-colocate test3

    source script.nu; cd test1; random-commits jj 8

init: rm
    ./target/release/jj re init test1
    ./target/release/jj re init test2 --bare
    ./target/release/jj re init test3

    source script.nu; cd test1; random-commits ../target/release/jj 8

step1:
    cd test1; ../target/release/jj re export ../test2

step1-merge:
    ./target/release/merge test2/op_heads/db test1/.jj/repo/op_heads/db
    ./target/release/merge test2/op_store/db test1/.jj/repo/op_store/db
    ./target/release/merge test2/store/db test1/.jj/repo/store/db

db1:
    ./target/release/db test1
    ./target/release/db test2 --bare

diff1:
    ./target/release/diff test1 914b2aa849a1329444f2ece7b4d9351ef4697de33d726e0a0ac8fe9e5b4995cd447bb9778c0500b56a0f6c3797a1d1bc1713fc9395cbef23945185a00f608c46 699ee35755802eae830ef590f31533be0625c10e78958e692ae6b3bf1eb6bd0b915b4941851a1fa0366b7b23fd3b81806a178e20d9e3732addaf70403702faf0

step2:
    cd test3; ../target/release/jj re import ../test2

step3:
    ./target/release/jj re clone test1 test4

log:
    ./target/release/log test1
    ./target/release/log test2 --bare
    ./target/release/log test3

append-git:
    source script.nu; cd test1; random-commits jj 100

append:
    source script.nu; cd test1; random-commits ../target/release/jj 100

length:
    (du test1/.jj/repo/op_heads | get physical | first) + \
    (du test1/.jj/repo/op_store | get physical | first) + \
    (du test1/.jj/repo/store | get physical | first)

gc-git:
    cd test1; jj util gc

gc:
    cd test1; ../target/release/jj util gc
