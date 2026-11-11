// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2

// Starts `cigar-lsp` (from PATH, or from the `cigarscript.server.path`
// setting) and connects the editor to it over stdio. No telemetry and no
// network: the only thing this does is start the program you installed.

import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;
let channel: vscode.OutputChannel | undefined;

function serverCommand(): string {
  const configured = vscode.workspace
    .getConfiguration("cigarscript")
    .get<string>("server.path");
  const trimmed = (configured ?? "").trim();
  return trimmed === "" ? "cigar-lsp" : trimmed;
}

async function start(): Promise<void> {
  const command = serverCommand();
  const run = { command, transport: TransportKind.stdio, args: [] as string[] };
  const serverOptions: ServerOptions = { run, debug: run };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "cigar" },
      { scheme: "file", language: "cig" },
      { scheme: "untitled", language: "cigar" },
      { scheme: "untitled", language: "cig" },
    ],
    outputChannel: channel,
  };
  const c = new LanguageClient(
    "cigarscript",
    "CigarScript",
    serverOptions,
    clientOptions,
  );
  client = c;
  try {
    await c.start();
  } catch (e) {
    client = undefined;
    const why = e instanceof Error ? e.message : String(e);
    void vscode.window.showErrorMessage(
      `CigarScript: could not start \`${command}\` (${why}). ` +
        "Install cigar-lsp so it is on your PATH, or set cigarscript.server.path.",
    );
  }
}

async function stop(): Promise<void> {
  const c = client;
  client = undefined;
  if (c) {
    try {
      await c.stop();
    } catch {
      // A server that already exited has nothing to stop.
    }
  }
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  channel = vscode.window.createOutputChannel("CigarScript");
  context.subscriptions.push(
    channel,
    vscode.commands.registerCommand("cigarscript.restartServer", async () => {
      await stop();
      await start();
    }),
  );
  await start();
}

export async function deactivate(): Promise<void> {
  await stop();
}
