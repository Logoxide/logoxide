# logoxide-mcp

An [MCP](https://modelcontextprotocol.io) server for a logoxide project, over stdio: an assistant
(Claude Code, Claude Desktop, any MCP client) can search catalogs, read and compare passages, map
references between verse numberings, and look into the project's packages, commentaries,
dictionaries, and cross-references. It's built on `logoxide-app`, as `lx` is, and reads the same
library and cache, so what one has parsed the other finds.

```sh
cargo install --path crates/logoxide-mcp      # the logoxide-mcp binary
```

## Setup

The project is chosen as `lx` chooses it: `--project <folder>`, else the current folder if it has
a `logoxide.toml`, else the library's own. `--library` and `--cache-dir` (or `LOGOXIDE_HOME`,
`LOGOXIDE_CACHE`, logoxide's `config.toml`) say where packages are, as for `lx`.

Claude Code:

```sh
claude mcp add logoxide -- logoxide-mcp --project ~/notes
# or, to let it install and import too:
claude mcp add logoxide -- logoxide-mcp --project ~/notes --allow-writes
```

Claude Desktop (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "logoxide": {
      "command": "logoxide-mcp",
      "args": ["--project", "/home/me/notes"]
    }
  }
}
```

Install the project's packages first (`lx install`), or start the server with `--allow-writes`
and ask for the `install` tool.

## Tools

Packages are named as `list_packages` gives them: the manifest's name, the id, the abbreviation,
or an alias, in any case.

| Tool | Arguments | Gives |
|---|---|---|
| `search_catalogs` | `query`, `registry?`, `limit?` (20, at most 100), `cursor?` | packages the project's registries and catalogs list: id, kind, source (git, registry, or an `import` name), rights, and their own words |
| `list_packages` | `cursor?` | the lock's packages: name, id, version, kind, on or off, installed, free |
| `package_info` | `package` | one package: books, verses, format, language, rights, license concerns, work, upstream |
| `read_passage` | `reference`, `bible?`, `labels?`, `cursor?` | a passage's text from one Bible, with its license |
| `compare_passage` | `reference`, `bibles?`, `labels?`, `cursor?` | the passage in each Bible's own numbering (mapped), what each lacks, the licenses |
| `map_reference` | `reference`, `from`, `to` | where a reference is in another numbering, and what has no counterpart |
| `show_summary` | `package`, `version?` | what a version holds (installed, or a release of its git repository) |
| `diff_versions` | `package`, `from`, `to?`, `cursor?` | verses reworded, moved, removed, added, joined, split; entries; patch, minor, or major |
| `cited_references` | `package`, `cursor?` | the passages a book's EPUBs cite, and how often |
| `read_notes` | `reference`, `commentary?`, `cursor?` | each commentary note covering the passage |
| `lookup_entry` | `key`, `dictionary?`, `cursor?` | a dictionary's entry (`G25`, `H430`, a headword) |
| `cross_references` | `reference`, `package?`, `cursor?` | the cross-reference lines covering any verse of the passage |
| `install` | `locked?` | with `--allow-writes`: installs what the manifest asks for |
| `import` | `source` | with `--allow-writes`: an import (`ebible:eng-kjv2006`) from the project's catalogs |

Each result is JSON (as structured content, and the same as text); a failure is a tool error in
words.

## What it returns, and what it won't

- **Untrusted text is marked**: everything a catalog, a manifest, or a package says (names,
  descriptions, licenses, titles, passages, notes, entries) is inside an `untrusted` object with
  its `source`, cleaned of control characters, terminal escapes, and bidirectional controls
  (`logoxide::text::printable`), and each field capped (300 characters for a name, 2,000 for a
  description, 16,000 for a note or entry). The server's instructions tell the client to treat
  it as data, never as instructions.
- **Every passage carries its license**: the package's license, rights, publisher, and authors,
  and whether it's known to be free. Text that isn't (no free SPDX license, and rights that don't
  say it's free) is limited to 500 verses a call and never more than half a book; past that the
  text is withheld, saying why. It's a limit on output, not enforcement: licenses are shown,
  never enforced, and you must check that you may use a text.
- **Pages**: a result longer than about 20,000 characters comes in pages; pass its `nextCursor`
  as `cursor` to get the next. A cursor is good only for the question it came from.
- **Read-only** unless started with `--allow-writes`: then `install` (what the manifest asks
  for) and `import` (only an entry of the project's own catalogs: never a URL or a path) are
  listed too, taking the same locks as `lx`.

See `doc/trust.md` for the rest of logoxide's trust model.

## Tests

`cargo test -p logoxide-mcp` runs each tool on the fixture project (`tests/fixtures`: two Bibles
numbered differently and a map between them, one of them under copyright, a commentary, a
dictionary, cross-references, a book made in the test) and a scripted client session: the binary
spawned, `initialize`, `tools/list`, and `tools/call` sent over stdio. When
`~/Development/logoxide-packages/kjv` and `bsb` exist, a second session installs and reads them.
