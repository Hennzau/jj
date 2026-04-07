# JJ with a ReDB backend

Thanks to the amazing crate `jj-cli`, it's possible to add functionalities and backends to the original `jj` cli!

This version of `jj` contains 100% of the upstream `jj` cli, with more functionalities:

- A native ReDB backed store for operations, heads and commits!
- Bare ReDB repositories (no `working-copy`), useful for remotes
- Export/Import your complete revision tree to remote repositories without having to name your branches, solve conflicts or describe commits!

# Caveats

The current ReDB backend is not ideal: it's slower than the `git` backend or the `simple_backend`, but it has the good property to store everything in a single `db` file, which makes it easy to export/import or sync.

I do not guaranty backward compatibility: you may loose your history (**NOT YOUR DATA**) between versions. You can't convert yet a `git` backed repo to a `redb` one nor the opposite.

This is purely experimental for folkes that likes to explore!

# Roadmap

No detailed roadmap, but I would really like to complexify this backend and make it more powerful. I'm not a specialist in data storage, compression etc... it's more like a learning project!

# Smart Protocol

`jj re export --client` (request an export from local to remote)

- send local heads to @stdout: message `Heads`
- listen on @stdin for remote heads (or abort if empty): message `Heads`
- send all needed data to @stdout: message `Pack`

`jj re export --server` (serve an export from remote to local)

- listen on @stdin for local heads: message `Heads`
- { compute what the client is missing }
- send up_to_date value to @stdout: message `bool`
- send all missing data to @stdout: message `Pack`

`jj re import --client` (request an import from remote to local)

- send local heads to @stdout: message: `Heads`
- listen on <stdin> for up_to_date value: message `bool`
- listen on <stdin> for all needed data: message `Pack`

`jj re import --server` (serve an import from local to remote)

- listen on @stdin for local heads: message `Heads`
- { compare local heads with operations }
- send remote heads (or empty to abort) to @stdout: message `Heads` 
- listen on @stdin for all needed data: message `Pack`

{ host1 }: `jj re export [destination on { host2 }]`

- if { host2 } is local: spawn `jj re import --server` at `[destination]`
- if { host2 } is ssh: spawn `ssh { host2 } bash -c "cd [destination]; jj re import --server"`
- spawn `jj re export --client` and pipe the stdin/out for communication

{ host1 }: `jj re import [destination on { host2 }]`

- if { host2 } is local: spawn `jj re export --server` at `[destination]`
- if { host2 } is ssh: spawn `ssh <host2> bash -c "cd [destination]; jj re export --server"`
- spawn `jj re import --client` and pipe the stdin/out for communication
