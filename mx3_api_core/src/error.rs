//! The body of a failed answer (PLT-23).

use serde::{Deserialize, Serialize};

/// What every mx3 endpoint answers with a non-success status: `{"error": "..."}`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ErrorBody {
    pub error: String,
}

/// The message in a failed answer's body: the `error` of an [ErrorBody], else the body as it is (the mail and
/// records endpoints answer plain text).
pub fn error_message(body: &str) -> String {
    match serde_json::from_str::<ErrorBody>(body) {
        Ok(e) => e.error,
        Err(_) => body.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_from_json_or_text() {
        assert_eq!(error_message(r#"{"error":"no such part"}"#), "no such part");
        assert_eq!(error_message("plain text"), "plain text");
    }
}
