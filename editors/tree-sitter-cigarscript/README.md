# tree-sitter-cigarscript

A [tree-sitter](https://tree-sitter.github.io) grammar for CigarScript
(`.cigar`) and CigScript (`.cig`), for syntax highlighting, folding and
structural selection in Neovim, Helix and Zed. Errors, hover, go to
definition and formatting are the language server's (`cigar-lsp`), not this
grammar's.

```
npm ci                    # tree-sitter-cli, pinned by package-lock.json
npx tree-sitter generate  # grammar.js -> src/parser.c
npx tree-sitter test      # test/corpus/*.txt
```

`queries/highlights.scm` uses the standard capture names (`@keyword`,
`@function`, `@type`, `@string`, ...), so an editor's own theme colors it.

## What it reads

The grammar of `docs/LANGUAGE.md` at every strength at once (an editor does not
know the strength a file will be checked at): CigScript's statements and
expressions, and `cigar`'s records, enums, interfaces, `impl`, `use`/`pub`,
`match`, `into`/`measure` and layers, ownership words, `parallel`, `channel`,
`within`, raw identifiers (`r#match`), types with `?` and generics, and
durations (`2s`).

## What it does not

- **It is a superset.** A newline ends a statement in the language; here it is
  whitespace, so a tree is made for text the checker would refuse. `cigar check`
  judges, this only reads.
- A line that starts `into for T = f` straight after a line that ends in an
  expression reads as `expr into for ...`. Put layer meanings after a
  declaration of another kind, or after a `;`.
- A contextual word used as a name in a mild file (`stick match = 1`) is read as
  the keyword. `cigar check` warns about those anyway (`W1234`).
- A call with explicit type arguments (`f<T>(x)`) is not read; only
  `channel<T>()` takes them.

## Using it

Neovim (nvim-treesitter) and Helix take a grammar from a local path or a
repository; point them at this folder and at `queries/`. Zed loads a grammar
through an extension. Each of them runs `tree-sitter generate` itself.
