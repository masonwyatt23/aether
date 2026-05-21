import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const config = vscode.workspace.getConfiguration("aether");
  const serverPath: string = config.get("serverPath") ?? "aether-lsp";
  const traceLevel: string = config.get("trace.server") ?? "off";

  // Resolve the server binary. If the path is relative, resolve it against the
  // first workspace folder so the user can write "./target/release/aether-lsp".
  let resolvedPath = serverPath;
  if (!path.isAbsolute(serverPath) && serverPath !== "aether-lsp") {
    const workspaceRoot =
      vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? "";
    resolvedPath = path.resolve(workspaceRoot, serverPath);
  }

  const serverOptions: ServerOptions = {
    command: resolvedPath,
    args: [],
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "aether" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.{ae,aev}"),
    },
    traceOutputChannel: vscode.window.createOutputChannel(
      "Aether Language Server Trace"
    ),
  };

  client = new LanguageClient(
    "aether",
    "Aether Language Server",
    serverOptions,
    clientOptions
  );

  // Honour the trace.server setting.
  if (traceLevel !== "off") {
    client.setTrace(traceLevel === "verbose" ? 2 : 1);
  }

  client.start();

  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (
        e.affectsConfiguration("aether.serverPath") ||
        e.affectsConfiguration("aether.trace.server")
      ) {
        vscode.window
          .showInformationMessage(
            "Aether: server configuration changed — reload window to apply.",
            "Reload"
          )
          .then((action) => {
            if (action === "Reload") {
              vscode.commands.executeCommand("workbench.action.reloadWindow");
            }
          });
      }
    })
  );
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
