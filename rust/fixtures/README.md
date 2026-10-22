# Test fixtures

Synthetic C55x programs for the Rust tests, with everything the tests read committed so
`cargo test` needs no TI tools. No firmware code, addresses or listings belong here.

Each directory NAME holds:

- `prog.asm`: the source (algebraic syntax), written by hand for the tests.
- `fixture.txt`: `origin 0x......`, the program byte address of `.text`.
- `prog.lst`: `dis55 -qq -r -g` of the linked program (the runtime listing flags), read
  with `c55x_core::frontend::listing::parse`.
- `prog.hex`: the loaded sections of the linked program. Per section there is a line
  `NAME 0xADDRESS SIZE`, then its bytes in hex, 32 per line.
- Expected results (`*.tsv`), when a test compares against frozen values. The header line
  of each file names where the values come from.

Regenerate or check `prog.lst` / `prog.hex` with the TI tools, in a work job:

    bin/c55x-contained --job work --env C55X_TI_BIN -- \
        build/rust-target/release/c55x-fixtures check      # or: write [NAME...]

`check` fails when a committed file differs from what the tools produce. The tool is
`rust/c55x_fixtures`. It finds the tools through `--ti DIR`, `$C55X_TI_BIN` or `PATH`.
