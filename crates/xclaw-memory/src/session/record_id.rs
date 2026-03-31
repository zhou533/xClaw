//! Unique identifier generation for transcript records.

/// Type alias for record identifiers (8-character nanoid, base62).
pub type RecordId = String;

/// Alphanumeric alphabet for nanoid generation (a-zA-Z0-9).
const ALPHABET: [char; 62] = [
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L',
    'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1', '2', '3', '4',
    '5', '6', '7', '8', '9',
];

/// Generate a new unique record identifier (8-character alphanumeric string).
pub fn generate_record_id() -> RecordId {
    nanoid::nanoid!(8, &ALPHABET)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_8_char_id() {
        let id = generate_record_id();
        assert_eq!(id.len(), 8);
    }

    #[test]
    fn contains_only_alphanumeric() {
        let id = generate_record_id();
        assert!(
            id.chars().all(|c| c.is_ascii_alphanumeric()),
            "id contains non-alphanumeric: {id}"
        );
    }

    #[test]
    fn two_ids_are_distinct() {
        let a = generate_record_id();
        let b = generate_record_id();
        assert_ne!(a, b, "two generated ids should differ");
    }
}
