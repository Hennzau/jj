set shell := ["nu", "-c"]

build:
    cargo build --release

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
