import path from "path";
import { ExtensionContext, workspace } from "vscode";
import { LanguageClient, LanguageClientOptions, ServerOptions } from "vscode-languageclient/node";

let client: LanguageClient;

export function activate(context: ExtensionContext) {
	// Path to your compiled Rust binary
	// ! In production, we need to bundle this
	const serverExecutable = context.asAbsolutePath(path.join("target", "debug", "toml_schema_extension_server.exe"));
	console.log("exec", serverExecutable);

	const serverOptions: ServerOptions = {
		run: { command: serverExecutable },
		debug: { command: serverExecutable },
	};

	const clientOptions: LanguageClientOptions = {
		// Register the server for TOML files
		documentSelector: [{ scheme: "file", language: "toml" }],
		synchronize: {
			fileEvents: workspace.createFileSystemWatcher("**/*.toml"),
		},
	};

	client = new LanguageClient("tomlSchemaLsp", "TOML Schema Language Server", serverOptions, clientOptions);

	// Starting the client also starts the Rust binary
	client.start();
}

export function deactivate(): Thenable<void> | undefined {
	if (!client) {
		return undefined;
	}

	return client.stop();
}
