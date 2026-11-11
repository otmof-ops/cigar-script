# CigarScript for VS Code

Language support for CigarScript (`.cigar`) and CigScript (`.cig`):

- syntax highlighting (a TextMate grammar, no server needed);
- errors as you type, hover (types, signatures, the kind of no, layers),
  go to definition, find references, completion and format document, all from
  `cigar-lsp`, which reads files with the same code `cigar check` does.

The extension starts `cigar-lsp` and nothing else. It sends no telemetry and
makes no network request of its own.

## Install the server

`cigar-lsp` ships beside `cigar`. It must be on your `PATH`, or named in the
`cigarscript.server.path` setting (a name or a full path):

```json
{ "cigarscript.server.path": "/home/you/.cigar/bin/cigar-lsp" }
```

Run **CigarScript: Restart language server** after changing it. If the program
is missing, the extension says so and highlighting still works.

`cigarscript.server.path` is ignored in a workspace you have not trusted: a
folder you opened should not choose which program runs.

## Settings

| setting | default | meaning |
|---|---|---|
| `cigarscript.server.path` | `cigar-lsp` | the server to start |
| `cigarscript.trace.server` | `off` | log editor-server messages (`messages`, `verbose`) |

## Build and install from source

Needs Node 18 or newer.

```
cd editors/vscode
npm ci
npm run compile
npm run package          # writes cigarscript-0.1.0.vsix
code --install-extension cigarscript-0.1.0.vsix
```

`npm ci` installs from `package-lock.json`. The one runtime dependency is
`vscode-languageclient`; the rest (TypeScript, the type definitions, `vsce`)
are build tools and are not in the package.

Format on save: set `"editor.formatOnSave": true` for the `[cigar]` and `[cig]`
languages; formatting is the same as `cigar fmt`.
