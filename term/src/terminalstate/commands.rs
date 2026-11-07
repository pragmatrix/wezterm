//! Records the shell's commands from the OSC 133 semantic prompt markers, so an embedder can tell
//! whether the shell waits at a prompt, which command runs, and how the last one ended.
//!
//! A shell integration sends `A` where the prompt starts, `B` where the user's input starts, `C`
//! where the command's output starts, and `D;<status>` when the command has finished. The engine
//! already marks every cell as prompt, input or output from them. This module adds what that leaves
//! out: where each command's input and output lie, and its exit status.
//!
//! The positions are those of the active screen. Shells send their markers on the primary screen,
//! so markers that arrive while the alternate screen is active are not supported.
//!
//! A shell started inside the terminal's shell, such as one on a remote host reached with `ssh`,
//! may send markers of its own. They count as the outer shell's, so the nested shell's first
//! prompt ends the outer command that started it.

use std::mem;
use std::ops::Range;

use crate::{SequenceNo, StableRowIndex};

/// What the terminal knows of its shell's commands. [`crate::TerminalState::shell_commands`]
/// returns it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellCommands {
    pub state: CommandState,
    /// The last command that finished, or `None` before the first one.
    pub last_finished: Option<FinishedCommand>,
}

/// Where the shell is between commands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CommandState {
    /// No OSC 133 marker has arrived yet, so the shell may have no integration.
    #[default]
    NoMarkers,
    /// The shell is between a command's end and its next input, which is where it draws its
    /// prompt.
    BetweenCommands,
    /// The shell shows its prompt, and the user's input starts at `input_start`.
    AtPrompt { input_start: StablePosition },
    /// A command runs. Its command line covers `input_rows`, and its output starts at
    /// `output_start`.
    Running {
        input_rows: Range<StableRowIndex>,
        output_start: StableRowIndex,
    },
}

/// A cell position in stable rows, which keep their index while lines scroll into the scrollback.
/// Scrolling inside a scroll region, and a resize that rewraps lines, move content between rows, so
/// a recorded position may then point at other content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StablePosition {
    pub row: StableRowIndex,
    pub column: usize,
}

/// A command that ended, either with `D` or with the next prompt's `A`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinishedCommand {
    /// The rows of the command line.
    pub input_rows: Range<StableRowIndex>,
    /// The rows of the command's output. Empty when the command wrote nothing.
    pub output_rows: Range<StableRowIndex>,
    /// The status `D` carried, or `None` when the next prompt started without a `D`. A `D` without
    /// a status, or with one that is no number, counts as status 0, because the escape parser
    /// reads it so.
    pub exit_status: Option<i32>,
    /// The terminal's sequence number when the command finished.
    pub seqno: SequenceNo,
}

impl ShellCommands {
    /// `A` or `N`: the prompt starts at the cursor. A command still running ended without `D`.
    pub(crate) fn prompt_started(&mut self, cursor: StablePosition, seqno: SequenceNo) {
        self.end_command(None, cursor, seqno);
    }

    /// `P`: a part of a prompt, such as a continuation line, which changes no state.
    pub(crate) fn prompt_continued(&mut self) {
        if self.state == CommandState::NoMarkers {
            self.state = CommandState::BetweenCommands;
        }
    }

    /// `B`: the user's input starts at the cursor. A continuation line, such as the one zsh shows
    /// after an unclosed quote, sends another `B` within the same prompt. The input still starts
    /// where the first `B` put it, so the shell does not look ready for a new command there.
    pub(crate) fn input_started(&mut self, cursor: StablePosition) {
        if !matches!(self.state, CommandState::AtPrompt { .. }) {
            self.state = CommandState::AtPrompt {
                input_start: cursor,
            };
        }
    }

    /// `C`: the command line was entered, and the command's output starts at the cursor, which
    /// sits on the line after the command line.
    pub(crate) fn output_started(&mut self, cursor: StablePosition) {
        let input_start = match self.state {
            CommandState::AtPrompt { input_start } => input_start.row,
            // Without `B`, the command line is unknown.
            _ => cursor.row,
        };
        self.state = CommandState::Running {
            input_rows: input_start..cursor.row.max(input_start),
            output_start: cursor.row,
        };
    }

    /// `D`: the running command finished with `status`, and its output ends at the cursor. A `D`
    /// without a running command, as shells send before their first prompt, records nothing.
    pub(crate) fn command_finished(
        &mut self,
        status: i32,
        cursor: StablePosition,
        seqno: SequenceNo,
    ) {
        self.end_command(Some(status), cursor, seqno);
    }

    /// Records the running command as finished, if one runs, and leaves the shell between
    /// commands.
    fn end_command(&mut self, exit_status: Option<i32>, cursor: StablePosition, seqno: SequenceNo) {
        let CommandState::Running {
            input_rows,
            output_start,
        } = mem::replace(&mut self.state, CommandState::BetweenCommands)
        else {
            return;
        };
        // Output that ends without a line break leaves the cursor on its last row.
        let output_end = cursor.row + StableRowIndex::from(cursor.column > 0);
        self.last_finished = Some(FinishedCommand {
            input_rows,
            output_rows: output_start..output_end.max(output_start),
            exit_status,
            seqno,
        });
    }
}
