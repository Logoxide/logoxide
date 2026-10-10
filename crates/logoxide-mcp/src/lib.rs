/*!
logoxide-mcp: an MCP server (the Model Context Protocol, over stdio) for a logoxide project, so an
assistant can search catalogs, read and compare passages, map references between numberings, and
look into the project's packages, commentaries, dictionaries, and cross-references. Built on
logoxide-app's [`Logoxide`](logoxide_app::Logoxide) and the core's projects, caches, and
summaries, as lx is.

Read-only unless started with `--allow-writes` (the owner's D14), which adds `install` and
`import` (from the project's catalogs only). Text from catalogs, manifests, and packages is
returned in `untrusted` objects, cleaned of terminal controls and capped ([`limits`]); passages
carry their license, and text that isn't free is capped (D15). See `doc/trust.md`
*/

pub mod limits;
pub mod server;
pub mod tools;

pub use server::Server;
pub use tools::Context;

#[cfg(test)]
mod tests;
