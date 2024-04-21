use std::env;

pub fn name() -> String {
    env::var("NAME").unwrap_or_else(|_| "Anonymous".to_string())
}
