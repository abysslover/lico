//! Text a person did not type (M24-P08). A chat's records keep more than the person's words: Claude Code stores a slash command as markup
//! (`<command-name>/model</command-name>`) and its output as another record, background tasks and teammates leave notices, and Orca types a wake-up
//! sentence into an agent's terminal (`You have 1 orchestration message. Run `orca orchestration check`.`), which the agent's own record then keeps as a prompt.
//! None of that is a message of the chat: it is not the title of a card, not a turn of its count and not a line of the transcript shown when it is resumed.
//! This module only judges text; it reads no file and knows no other module, so every reader of a chat can use the same rule.

/// Records that begin like this were written by a program, not typed: a command's output, error and caveat, a background task's notice, a relay from a teammate.
const MARKUP: &[&str] = &["<local-command-", "<task-notification>", "<teammate-message"];

/// Orca's wake-up sentence: `You have`, a number, ` orchestration message` (also `messages`), and whatever follows it (a preview of the message, written by someone else).
pub fn orchestration_wakeup(text: &str) -> bool {
    let Some(rest) = text.trim_start().strip_prefix("You have ") else { return false };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && rest[digits..].starts_with(" orchestration message")
}

/// Whether a line begins the record of a slash command (`<command-name>`, `<command-message>`, `<command-args>`).
fn command_markup(text: &str) -> bool {
    text.trim_start().starts_with("<command-")
}

/// What is between `open` and `close` in `text`, the first time.
fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let from = text.find(open)? + open.len();
    let len = text[from..].find(close)?;
    Some(&text[from..from + len])
}

/// The words of a user record that stand for the person, or `None` when there are none: markup and the wake-up sentence are not words, a command with words after
/// its name is the line the person typed (`/orchestration fix the bug`), a command without words (`/model`) is a control and says nothing. Any other text is kept as it is.
pub fn person_text(text: &str) -> Option<String> {
    let head = text.trim_start();
    if orchestration_wakeup(head) || MARKUP.iter().any(|m| head.starts_with(m)) {
        return None;
    }
    if command_markup(head) {
        let args = between(head, "<command-args>", "</command-args>")?.trim();
        let name = between(head, "<command-name>", "</command-name>")?.trim();
        if args.is_empty() || name.is_empty() {
            return None;
        }
        return Some(format!("{}{name} {args}", if name.starts_with('/') { "" } else { "/" }));
    }
    Some(text.to_string())
}

/// Whether a line of a prompt can never be the title of a chat: a slash command (`/model sonnet`), a line the host wrote (`[from session …]`, `[a long line …]`),
/// the markup of a program, Orca's wake-up sentence. The line is trimmed.
pub fn not_a_title_line(line: &str) -> bool {
    line.starts_with('/')
        || line.starts_with("[from session ")
        || line.starts_with("[a long line ")
        || command_markup(line)
        || MARKUP.iter().any(|m| line.starts_with(m))
        || orchestration_wakeup(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wake_up_sentence_is_known_by_its_shape_and_not_by_one_wording() {
        for yes in [
            "You have 1 orchestration message.",
            "You have 12 orchestration messages. Run `orca orchestration check --terminal term_1`.",
            "  \n\tYou have 3 orchestration message. A preview of it follows",
        ] {
            assert!(orchestration_wakeup(yes), "{yes:?}");
        }
        for no in ["", "You have orchestration message", "You have  1 orchestration message", "You have  orchestration message", "You have 1 orchestrations", "you have 1 orchestration message", "Do you have 1 orchestration message?", "You have 1 message", "You have 1"] {
            assert!(!orchestration_wakeup(no), "{no:?}");
        }
    }

    #[test]
    fn a_command_is_the_words_the_person_typed_or_nothing_and_markup_is_nothing() {
        let record = |name: &str, args: &str| format!("<command-message>x</command-message>\n  <command-name>{name}</command-name>\n  <command-args>{args}</command-args>");
        assert_eq!(person_text(&record("/model", "")), None);
        assert_eq!(person_text(&record("/model", "  \n ")), None, "white space is no words");
        assert_eq!(person_text(&record("/orchestration", " fix the bug ")).as_deref(), Some("/orchestration fix the bug"));
        assert_eq!(person_text(&record("model", "opus")).as_deref(), Some("/model opus"), "the name is shown with its slash");
        assert_eq!(person_text("<command-name>/model</command-name>"), None, "a record with no arguments element says nothing");
        assert_eq!(person_text("<command-name></command-name><command-args>x</command-args>"), None, "no name");
        assert_eq!(person_text("<command-name>/x</command-name><command-args>never closed"), None, "a damaged record says nothing");
        for markup in ["<local-command-stdout>Kept</local-command-stdout>", "  <local-command-caveat>x", "<task-notification>x", "<teammate-message id=\"1\">x"] {
            assert_eq!(person_text(markup), None, "{markup}");
        }
        assert_eq!(person_text("You have 1 orchestration message. Run `orca orchestration check`."), None);
        for said in ["a question", "  indented words", "You have 3 apples", "about <command-name> in general", "<b>bold</b>", "<task-notifications are fine to discuss"] {
            assert_eq!(person_text(said).as_deref(), Some(said), "{said}: the text is returned as it is, not trimmed");
        }
    }

    #[test]
    fn a_line_that_cannot_title_a_chat_is_a_slash_a_line_of_the_host_markup_or_the_wake_up_sentence() {
        for line in ["/model sonnet", "[from session abc]", "[a long line 5000 characters]", "<command-name>/model</command-name>", "<command-args></command-args>", "<local-command-stdout>x", "<task-notification>x", "<teammate-message id=\"1\">x", "You have 1 orchestration message."] {
            assert!(not_a_title_line(line), "{line}");
        }
        for line in ["the real question", "You have 3 apples", "[a note]", "a / b", "<b>bold</b>"] {
            assert!(!not_a_title_line(line), "{line}");
        }
    }
}
