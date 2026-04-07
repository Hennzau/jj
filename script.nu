def random-text [len: int] {
    (random chars --length $len)
}

def random-filename [] {
    $"file_(random int 1..1000000000).txt"
}

def random-message [] {
    let words = ["fix", "feat", "refactor", "update", "cleanup", "wip", "test"]
    let count = (random int 1..5) + 1
    ($words | shuffle | first $count | str join " ")
}

def random-commits [jj_cmd, total: int] {
    let start = (date now)

    for i in 0..($total - 1) {
        let files = random int 1..10

        for j in 1..$files {
            let filename = (random-filename)
            let content = (random-text 1000)
            $content | save --force $filename
        }

        let msg = (random-message)
        ^$jj_cmd describe -m $msg
        ^$jj_cmd new
    }

    let end = (date now)
    let duration = ($end - $start)

    print $"Execution time: ($duration)"
}
