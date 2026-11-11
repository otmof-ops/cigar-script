# Editors

Three parts, each independent of the others (#16):

| part | what it gives | where |
|---|---|---|
| `cigar-lsp` | errors as you type, hover, go to definition, references, completion, format document | any editor that starts a language server |
| VS Code extension | highlighting, and `cigar-lsp` started for you | `editors/vscode/` |
| tree-sitter grammar | highlighting, folding, structural selection | `editors/tree-sitter-cigarscript/` |

`cigar-lsp` ships beside `cigar` ([commands](commands.md#cigar-lsp)); the
other two are about starting it and coloring text.

## VS Code

`editors/vscode/` is an extension for `.cigar` and `.cig`: a TextMate grammar
for highlighting, and a client that starts `cigar-lsp` from your `PATH`, or
from the `cigarscript.server.path` setting. It needs one runtime dependency,
`vscode-languageclient`, sends no telemetry and makes no network request.

```
cd editors/vscode
npm ci && npm run compile && npm run package
code --install-extension cigarscript-0.1.0.vsix
```

To format on save, set `"editor.formatOnSave": true` for `[cigar]` and `[cig]`.
Its README has the settings.

## Any editor with a language server

Start `cigar-lsp` (no arguments, stdio) for the language ids `cigar` and
`cig`.

Neovim:

```lua
vim.filetype.add({ extension = { cigar = "cigar", cig = "cig" } })
vim.lsp.start({ name = "cigar-lsp", cmd = { "cigar-lsp" },
                root_dir = vim.fs.dirname(vim.api.nvim_buf_get_name(0)) })
```

Helix (`languages.toml`):

```toml
[language-server.cigar-lsp]
command = "cigar-lsp"

[[language]]
name = "cigar"
scope = "source.cigarscript"
file-types = ["cigar", "cig"]
comment-token = "#"
indent = { tab-width = 2, unit = "  " }
language-servers = ["cigar-lsp"]
```

Emacs (`eglot`):

```elisp
(add-to-list 'eglot-server-programs '((cigar-mode cig-mode) . ("cigar-lsp")))
```

Formatting is the server's `textDocument/formatting`, the same as `cigar fmt`.

## tree-sitter

`editors/tree-sitter-cigarscript/` is a grammar for highlighting, folding and
structural selection in Neovim (nvim-treesitter), Helix and Zed. It is the
grammar of [LANGUAGE.md](LANGUAGE.md) at every strength at once, with
`queries/highlights.scm` and a corpus of tests.

```
cd editors/tree-sitter-cigarscript
npm ci
npx tree-sitter generate
npx tree-sitter test
```

It is a superset on purpose (a tree for text the checker refuses), and it
leaves out three things; they are listed in its README. Errors are
`cigar-lsp`'s, not the grammar's.

## CI

`.github/workflows/editors.yml` builds the grammar (`tree-sitter generate`
and `test`) and the extension (`npm ci`, `npm run compile`, `vsce package`)
on Linux, and runs `cigar fmt --check` on this repository's own `.cigar` and
`.cig` files (not the test fixtures, whose errors name exact columns).
