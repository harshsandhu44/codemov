/// Estimates token count using chars/4 approximation.
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}
