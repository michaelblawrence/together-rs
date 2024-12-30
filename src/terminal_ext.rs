use std::borrow::Cow;

use crate::{config, errors::TogetherResult, log, manager, process, terminal};

pub trait TerminalExt {
    fn select_single_process<'a>(
        prompt: &'a str,
        sender: &'a manager::ProcessManagerHandle,
        list: &'a [process::ProcessId],
    ) -> TogetherResult<Option<&'a process::ProcessId>>;

    fn select_single_command<'a>(
        prompt: &'a str,
        sender: &'a manager::ProcessManagerHandle,
        list: &'a [config::commands::CommandConfig],
    ) -> TogetherResult<Option<&'a str>>;

    fn select_single_command_with_running<'a>(
        prompt: &'a str,
        sender: &'a manager::ProcessManagerHandle,
        list: &'a [config::commands::CommandConfig],
        running: &'a [process::ProcessId],
    ) -> TogetherResult<Option<&'a str>>;

    fn select_single_recipe<'a>(
        prompt: &'a str,
        sender: &'a manager::ProcessManagerHandle,
        list: &'a [String],
    ) -> TogetherResult<Option<&'a String>>;

    fn select_multiple_commands<'a>(
        prompt: &'a str,
        sender: &'a manager::ProcessManagerHandle,
        list: &'a [String],
    ) -> TogetherResult<Vec<&'a String>>;

    fn select_multiple_recipes<'a>(
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
        list: &'a [config::commands::CommandConfig],
    ) -> TogetherResult<Option<&'a str>> {
        if list.is_empty() {
            log!("No commands available...");
            return Ok(None);
        }
        let commands = list
            .iter()
            .map(|c| c.alias().unwrap_or(c.as_str()))
            .collect::<Vec<_>>();
        let command = terminal::Terminal::select_single_index(prompt, &commands).map(|index| {
            let command = list.get(index).unwrap();
            command.as_str()
        });
        Ok(command)
    }

    fn select_single_command_with_running<'a>(
        prompt: &'a str,
        _sender: &'a manager::ProcessManagerHandle,
        list: &'a [config::commands::CommandConfig],
        running: &'a [process::ProcessId],
    ) -> TogetherResult<Option<&'a str>> {
        if list.is_empty() {
            log!("No commands available...");
            return Ok(None);
        }
        let commands = list
            .iter()
            .map(
                |c| match running.iter().filter(|p| c.matches(p.command())).count() {
                    0 => Cow::from(c.alias().unwrap_or(c.as_str())),
                    // format: "command (x running)" with gray color for parentheses
                    x => format!(
                        "{} \x1b[90m({} running)\x1b[0m",
                        c.alias().unwrap_or(c.as_str()),
                        x
                    )
                    .into(),
                },
            )
            .collect::<Vec<_>>();
        let command = terminal::Terminal::select_single_index(prompt, &commands).map(|index| {
            let command = list.get(index).unwrap();
            command.as_str()
        });
        Ok(command)
    }

    fn select_single_recipe<'a>(
        prompt: &'a str,
        _sender: &'a manager::ProcessManagerHandle,
        list: &'a [String],
    ) -> TogetherResult<Option<&'a String>> {
        if list.is_empty() {
            log!("No recipes available...");
            return Ok(None);
        }
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

    fn select_multiple_recipes<'a>(
        prompt: &'a str,
        _sender: &'a manager::ProcessManagerHandle,
        list: &'a [String],
    ) -> TogetherResult<Vec<&'a String>> {
        if list.is_empty() {
            log!("No recipes available...");
            return Ok(vec![]);
        }
        let recipes = terminal::Terminal::select_multiple(prompt, list);
        if recipes.is_empty() {
            log!("No recipes selected...");
        }
        Ok(recipes)
    }
}
