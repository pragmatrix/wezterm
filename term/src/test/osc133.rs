//! The commands the engine records from OSC 133 markers.

use super::*;
use k9::assert_equal as assert_eq;

const PROMPT: &str = "\x1b]133;A\x07$ \x1b]133;B\x07";

fn finished(term: &TestTerm) -> FinishedCommand {
    term.shell_commands().last_finished.clone().unwrap()
}

#[test]
fn a_command_records_its_rows_and_exit_status() {
    let mut term = TestTerm::new(10, 20, 100);
    term.print(PROMPT);
    assert_eq!(
        term.shell_commands().state,
        CommandState::AtPrompt {
            input_start: StablePosition { row: 0, column: 2 }
        }
    );

    term.print("ls\r\n\x1b]133;C\x07");
    assert_eq!(
        term.shell_commands().state,
        CommandState::Running {
            input_rows: 0..1,
            output_start: 1
        }
    );

    term.print("a\r\nb\r\n\x1b]133;D;2\x07");
    let command = finished(&term);
    assert_eq!(command.input_rows, 0..1);
    assert_eq!(command.output_rows, 1..3);
    assert_eq!(command.exit_status, Some(2));
    assert_eq!(term.shell_commands().state, CommandState::BetweenCommands);
}

/// A continuation line, such as zsh's after an unclosed quote, sends another `B` within the same
/// prompt, which leaves the input's start where the first `B` put it.
#[test]
fn a_continuation_line_keeps_the_input_start() {
    let mut term = TestTerm::new(10, 20, 100);
    term.print(PROMPT);
    term.print("echo \"a\r\n\x1b]133;P\x07> \x1b]133;B\x07");
    assert_eq!(
        term.shell_commands().state,
        CommandState::AtPrompt {
            input_start: StablePosition { row: 0, column: 2 }
        }
    );
}

#[test]
fn output_without_a_final_line_break_ends_on_the_cursor_row() {
    let mut term = TestTerm::new(10, 20, 100);
    term.print(PROMPT);
    term.print("printf x\r\n\x1b]133;C\x07x\x1b]133;D;0\x07");
    assert_eq!(finished(&term).output_rows, 1..2);
}

#[test]
fn a_status_without_a_running_command_records_nothing() {
    let mut term = TestTerm::new(10, 20, 100);
    term.print("\x1b]133;D;0\x07");
    assert_eq!(term.shell_commands().last_finished, None);
    assert_eq!(term.shell_commands().state, CommandState::BetweenCommands);
}

#[test]
fn a_new_prompt_ends_a_running_command_without_a_status() {
    let mut term = TestTerm::new(10, 20, 100);
    term.print(PROMPT);
    term.print("ls\r\n\x1b]133;C\x07a\r\n");
    term.print(PROMPT);
    let command = finished(&term);
    assert_eq!(command.output_rows, 1..2);
    assert_eq!(command.exit_status, None);
}
