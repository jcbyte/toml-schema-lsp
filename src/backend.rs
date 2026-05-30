use tower_lsp::Client;

mod lsp;
mod util;
mod validation;

pub struct Backend {
    client: Client,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}
