# TOML Schema LSP

TOML Schema Language Server Protocol (LSP) written in Rust, and VS Code extension.
Using [TOML Schema](https://github.com/brunoborges/toml-schema) by [@brunoborges](https://github.com/brunoborges).

## Features

- **TOML Schema Validation:** Full compliance with, and using the [TOML Schema](https://github.com/brunoborges/toml-schema) specification.
- **Real-Time Diagnostics:** Validates TOML files on the fly as you type.
- **Local Schema Support:** Uses local `.tosd` files to define schemas.
- **Remote Schema Support:** Uses `https://` schema URLs, see [Remote Schemas](#remote-schemas).

## Limitations

- **TOML Version:** TOML Schema is currently based strictly on TOML 1.0.0.
- **Error Positioning:** The underlying Rust implementation of TOML Schema validation currently only returns paths and string messages, no error type/tokens. Because of this, editor diagnostics/errors are placed directly on item values rather than pinpointing specific syntax tokens.

## Roadmap

- [ ] **Automatic TOSD Validation:** Implement validation for `.tosd` files using TOML Schema's [Self-Schema](https://github.com/brunoborges/toml-schema/blob/main/toml-schema.tosd).
- [ ] **Caching:** Implement caching (and file watching) for `.tosd` files to prevent re-fetching (disk I/O) on every modification.

## Remote Schemas

`toml-schema.location` can be an `https://` URL. As a TOML file (and the server behind the URL) is untrusted, nothing is downloaded until you allow the host. The first time a host is used, you are asked, and **Allow** adds it to your user settings (VS Code only, other clients keep the answer until the server restarts). Hosts can also be allowed directly in your **user** settings (workspace settings cannot allow hosts):

```json
"tomlSchema.remote.allowedHosts": ["example.com"]
```

- Only `https` is supported. Redirects (max 3) are only followed within the same host, and URLs with credentials are rejected.
- Connect timeout 5s, total timeout 10s, response limited to 1 MiB.
- No cookies, authorisation or proxy settings are used.
- Schemas are cached in memory for 5 minutes (revalidated with `ETag`), and an outdated schema is used while offline.
- Other editors (e.g. Neovim) pass the same settings as LSP `settings` (or `initializationOptions`): `{ tomlSchema = { remote = { allowedHosts = { 'example.com' } } } }`. Never source them from files inside the opened project.
- Set `tomlSchema.remote.enabled` to `false` to disable remote schemas completely.

## Development

The project consists of a Rust backend LSP server, and a TypeScript VS Code extension client.

### Local Development

To develop locally, start file watchers for both the frontend and backend:

```bash
# Watch and build the VS Code client
npm run dev

# Watch and run the Rust LSP server
cargo watch -x run
```

**note:** In development, these communicate via TCP server (`localhost:9260`) as opposed to stdin/stdout.

Open this project in VS Code, press `F5` and select **"VS Code Extension Development"** debugger to launch a host window with the extension active.

### Packaging

To package the VS Code extension into a `.vsix` bundle:

```bash
npx @vscode/vsce package
```

## Licence

[Apache License 2.0](LICENSE)
