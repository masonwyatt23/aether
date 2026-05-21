//! JSON emission for the Aether AST.
//!
//! Produces a schema-versioned envelope:
//!   `{ "version": "0.1", "module": <Module> }`

use aether_ast::Module;

/// Serialize `m` to a JSON string.  Pass `pretty = true` for indented output.
pub fn emit(m: &Module, pretty: bool) -> String {
    #[derive(serde::Serialize)]
    struct Envelope<'a> {
        version: &'static str,
        module: &'a Module,
    }

    let env = Envelope { version: "0.1", module: m };
    if pretty {
        serde_json::to_string_pretty(&env).expect("AST serialization is infallible")
    } else {
        serde_json::to_string(&env).expect("AST serialization is infallible")
    }
}
