use crate::{errors::TogetherResult, log, manager, process, terminal};

pub trait TerminalExt {
    fn select_single_process<'a>(
        prompt: &'a str,
        sender: &'a manager::ProcessManagerHandle,
        list: &'a [process::ProcessId],
    ) -> TogetherResult<Option<&'a process::ProcessId>>;

    fn select_single_command<'a>(
        prompt: &'a str,
        sender: &'a manager::ProcessManagerHandle,
        list: &'a [String],
    ) -> TogetherResult<Option<&'a String>>;

    fn select_multiple_commands<'a>(
        prompt: &'a str,
        sender: &'a manager::ProcessManagerHandle,
        list: &'a [String],
    ) -> TogetherResult<Vec<&'a String>>;
}

impl TerminalExt for terminal::Terminal {
    fn select_single_process<'a>(
        prompt: &'a str,
        _sender: &'a manager::ProcessManagerHandle,
        list: &'a [process::ProcessId],
    ) -> TogetherResult<Option<&'a process::ProcessId>> {
        let command = terminal::Terminal::select_single(prompt, list);
        Ok(command)
    }

    fn select_single_command<'a>(
        prompt: &'a str,
        _sender: &'a manager::ProcessManagerHandle,
        list: &'a [String],
    ) -> TogetherResult<Option<&'a String>> {
        let command = terminal::Terminal::select_single(prompt, list);
        Ok(command)
    }

    fn select_multiple_commands<'a>(
        prompt: &'a str,
        _sender: &'a manager::ProcessManagerHandle,
        list: &'a [String],
    ) -> TogetherResult<Vec<&'a String>> {
        let commands = terminal::Terminal::select_multiple(prompt, list);
        if commands.is_empty() {
            log!("No commands selected...");
        }
        Ok(commands)
    }
}
