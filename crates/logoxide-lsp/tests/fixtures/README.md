`complete.txt` is a copy of topos's `crates/topos-lib/tests/cases/complete.txt`, the completion
cases topos and this server share, kept here so the tests build without a topos checkout beside
logoxide. Copy it again when topos's changes:

```sh
cp ../topos/crates/topos-lib/tests/cases/complete.txt crates/logoxide-lsp/tests/fixtures/
```
