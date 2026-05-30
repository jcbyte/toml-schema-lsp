import net from "net";
import path from "path";
import { ExtensionContext, ExtensionMode, workspace } from "vscode";
import { LanguageClient, LanguageClientOptions, ServerOptions } from "vscode-languageclient/node";

let client: LanguageClient;

export function activate(context: ExtensionContext) {
	let serverOptions: ServerOptions;

	if (context.extensionMode == ExtensionMode.Development) {
		// In development, connect to a running LSP rather than starting one up
		serverOptions = () => {
			return new Promise((resolve) => {
				// Use magic port number 9260, this is synced in `main.rs`
				const client = net.connect({ port: 9260 }, () => {
					resolve({
						reader: client,
						writer: client,
					});
				});
			});
		};
	} else {
		// In production, startup the server binary and communicate with stdin/stdout
		const isWindows = process.platform === "win32";
		const binName = isWindows ? "toml_schema_extension_server.exe" : "toml_schema_extension_server";
		const binPath = context.asAbsolutePath(path.join("server", binName));
		serverOptions = {
			run: { command: binPath },
			debug: { command: binPath },
		};
	}

	const clientOptions: LanguageClientOptions = {
		// Register the server for TOML files
		documentSelector: [{ scheme: "file", language: "toml" }],
		synchronize: {
			fileEvents: workspace.createFileSystemWatcher("**/*.toml"),
		},
	};

	client = new LanguageClient("tomlSchemaLsp", "TOML Schema Language Server", serverOptions, clientOptions);
	// Starting the client will also start the Rust binary (in prod)
	client.start();
}

export function deactivate(): Thenable<void> | undefined {
	if (!client) {
		return undefined;
	}

	return client.stop();
}
