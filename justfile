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

rm:
    rm -rf test*

init-git: rm
    jj git init --no-colocate test1
    jj git init --no-colocate test2
    jj git init --no-colocate test3

    source script.nu; cd test1; random-commits jj 8

init: rm
    ./target/release/jj init test1
    ./target/release/jj init test2 --bare
    ./target/release/jj init test3

    source script.nu; cd test1; random-commits ../target/release/jj 8

step1:
    cd test1; ../target/release/jj export ../test2

step2:
    cd test3; ../target/release/jj import ../test2

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
