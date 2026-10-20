//! Summary prompts, verbatim from Electron's `src/service/summarize.js`,
//! and the KTD3 chunk budget.

/// Output cap for each chunk's intermediate note (KTD3).
pub const CHUNK_NOTE_TOKENS: usize = 350;

/// Slack for tokens that merge differently where the chunk meets the
/// surrounding prompt text.
const BOUNDARY_SLACK: usize = 16;

/// `${basePrompt}\n<transcript>\n${chunk}\n</transcript>`
pub fn chunk_prompt(base: &str, chunk: &str) -> String {
    format!("{base}\n<transcript>\n{chunk}\n</transcript>")
}

/// The Electron combine prompt, with notes joined by `\n\n---\n\n`.
pub fn combine_prompt(base: &str, notes: &[String]) -> String {
    format!(
        "{base}\n\nYou have created notes for {} segments of the same session. \
         Combine them into a single, cohesive note following the format and \
         requirements above:\n\n{}",
        notes.len(),
        notes.join("\n\n---\n\n")
    )
}

/// Transcript tokens one chunk may hold: the context minus the templated
/// prompt around it and the chunk's output cap. `None` when the prompt
/// alone leaves no room.
pub fn chunk_budget(context_tokens: usize, empty_prompt_tokens: usize) -> Option<usize> {
    context_tokens
        .checked_sub(empty_prompt_tokens + CHUNK_NOTE_TOKENS + BOUNDARY_SLACK)
        .filter(|&b| b > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_match_the_electron_format_strings() {
        assert_eq!(
            chunk_prompt("BASE", "a b c"),
            "BASE\n<transcript>\na b c\n</transcript>"
        );
        assert_eq!(
            combine_prompt("BASE", &["one".into(), "two".into()]),
            "BASE\n\nYou have created notes for 2 segments of the same session. Combine \
             them into a single, cohesive note following the format and requirements \
             above:\n\none\n\n---\n\ntwo"
        );
    }

    #[test]
    fn budget_leaves_room_for_prompt_and_output() {
        assert_eq!(chunk_budget(8192, 600), Some(8192 - 600 - 350 - 16));
        assert_eq!(chunk_budget(900, 600), None);
    }
}
