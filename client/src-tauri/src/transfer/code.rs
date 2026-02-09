use rand::Rng;

use crate::error::{AppError, AppResult};

const WORDLIST: &str = include_str!("../../../wordlist.txt");

/// A human-friendly transfer code: "{digit}-{word}-{word}"
#[derive(Debug, Clone)]
pub struct TransferCode {
    pub digit: u8,
    pub word1: String,
    pub word2: String,
}

impl TransferCode {
    /// Generate a random transfer code.
    pub fn generate() -> Self {
        let mut rng = rand::rng();
        let words = wordlist();
        let digit = rng.random_range(0..10u8);
        let word1 = words[rng.random_range(0..words.len())].to_string();
        let word2 = words[rng.random_range(0..words.len())].to_string();
        Self {
            digit,
            word1,
            word2,
        }
    }

    /// Format as "7-guitar-palace"
    pub fn to_code_string(&self) -> String {
        format!("{}-{}-{}", self.digit, self.word1, self.word2)
    }

    /// Parse a code string like "7-guitar-palace"
    pub fn parse(code: &str) -> AppResult<Self> {
        let parts: Vec<&str> = code.trim().splitn(3, '-').collect();
        if parts.len() != 3 {
            return Err(AppError::InvalidCode(
                "expected format: digit-word-word".into(),
            ));
        }

        let digit: u8 = parts[0]
            .parse()
            .map_err(|_| AppError::InvalidCode("first part must be a digit 0-9".into()))?;

        if digit > 9 {
            return Err(AppError::InvalidCode("digit must be 0-9".into()));
        }

        let words = wordlist();
        let word1 = parts[1].to_lowercase();
        let word2 = parts[2].to_lowercase();

        if !words.contains(&word1.as_str()) {
            return Err(AppError::InvalidCode(format!(
                "unknown word: '{word1}'"
            )));
        }
        if !words.contains(&word2.as_str()) {
            return Err(AppError::InvalidCode(format!(
                "unknown word: '{word2}'"
            )));
        }

        Ok(Self {
            digit,
            word1,
            word2,
        })
    }
}

impl std::fmt::Display for TransferCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_code_string())
    }
}

fn wordlist() -> Vec<&'static str> {
    WORDLIST
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// Get a copy of the wordlist for the frontend.
pub fn get_wordlist() -> Vec<String> {
    wordlist().into_iter().map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wordlist_has_256_words() {
        let words = wordlist();
        assert_eq!(words.len(), 256, "wordlist must contain exactly 256 words");
    }

    #[test]
    fn test_wordlist_no_duplicates() {
        let words = wordlist();
        let mut seen = std::collections::HashSet::new();
        for w in &words {
            assert!(seen.insert(w), "duplicate word: {w}");
        }
    }

    #[test]
    fn test_generate_and_parse_roundtrip() {
        for _ in 0..50 {
            let code = TransferCode::generate();
            let s = code.to_code_string();
            let parsed = TransferCode::parse(&s).unwrap();
            assert_eq!(parsed.digit, code.digit);
            assert_eq!(parsed.word1, code.word1);
            assert_eq!(parsed.word2, code.word2);
        }
    }

    #[test]
    fn test_parse_invalid_format() {
        assert!(TransferCode::parse("invalid").is_err());
        assert!(TransferCode::parse("abc-guitar-palace").is_err());
        assert!(TransferCode::parse("7-notaword-palace").is_err());
        assert!(TransferCode::parse("10-guitar-palace").is_err());
    }
}
