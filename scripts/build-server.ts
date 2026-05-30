import { execSync } from "child_process";
import fs from "fs";
import path from "path";

function build() {
	// Build the Rust project for production
	console.log("🦀 Building Rust LSP");
	execSync("cargo build --release", { stdio: "inherit" });

	const isWindows = process.platform === "win32";
	const binName = isWindows ? "toml_schema_extension_server.exe" : "toml_schema_extension_server";

	const srcPath = path.join(__dirname, "..", "target", "release", binName);
	const distDir = path.join(__dirname, "..", "server");
	const distPath = path.join(distDir, binName);

	// Ensure the destination server directory exists
	if (!fs.existsSync(distDir)) {
		fs.mkdirSync(distDir, { recursive: true });
	}

	console.log(`🚚 Moving LSP to production directory`);
	fs.copyFileSync(srcPath, distPath);

	// On Unix systems, make sure the binary is executable
	if (!isWindows) {
		fs.chmodSync(distPath, "755");
	}

	console.log("✅ Complete!");
}

build();
