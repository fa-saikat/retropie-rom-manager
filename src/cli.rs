//! Command-line options. Parsed before GPUI starts, so `--help` and
//! `--version` work without a display.

use std::fmt;

#[derive(Debug, Default, PartialEq)]
pub struct Options {
    pub fullscreen: bool,
    pub theme: Option<Theme>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Run(Options),
    Help,
    Version,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Unexpected(String),
    MissingValue(&'static str),
    InvalidTheme(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Unexpected(arg) => write!(f, "unexpected argument '{arg}'"),
            Error::MissingValue(flag) => write!(f, "'{flag}' needs a value"),
            Error::InvalidTheme(value) => write!(f, "invalid value '{value}' for '--theme': expected light or dark"),
        }
    }
}

pub const USAGE: &str = "Usage: retropie-rom-manager [OPTIONS]";

pub fn help() -> String {
    format!(
        "Install, browse and remove RetroPie ROMs.

{USAGE}

Options:
      --fullscreen     Start in fullscreen. F11 toggles it while running.
      --theme <THEME>  Start in the light or dark theme [values: light, dark]
  -h, --help           Print this help and exit
  -V, --version        Print the version and exit"
    )
}

pub fn version() -> String {
    format!("retropie-rom-manager {}", env!("CARGO_PKG_VERSION"))
}

/// Parse the arguments after the program name. `--help` and `--version`
/// win over everything else, as in most tools.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, Error> {
    let mut options = Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag.to_string(), Some(value.to_string())),
            _ => (arg.clone(), None),
        };
        match flag.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--fullscreen" if inline.is_none() => options.fullscreen = true,
            "--theme" => {
                let value = inline.or_else(|| args.next()).ok_or(Error::MissingValue("--theme"))?;
                options.theme = Some(match value.as_str() {
                    "light" => Theme::Light,
                    "dark" => Theme::Dark,
                    _ => return Err(Error::InvalidTheme(value)),
                });
            }
            _ => return Err(Error::Unexpected(arg)),
        }
    }
    Ok(Command::Run(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Command, Error> {
        parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_arguments_runs_with_defaults() {
        assert_eq!(parse_strs(&[]), Ok(Command::Run(Options::default())));
    }

    #[test]
    fn fullscreen_and_theme() {
        let expected = Command::Run(Options { fullscreen: true, theme: Some(Theme::Dark) });
        assert_eq!(parse_strs(&["--fullscreen", "--theme", "dark"]), Ok(expected));
        let expected = Command::Run(Options { fullscreen: false, theme: Some(Theme::Light) });
        assert_eq!(parse_strs(&["--theme=light"]), Ok(expected));
    }

    #[test]
    fn help_and_version_win() {
        assert_eq!(parse_strs(&["--fullscreen", "-h"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["--version", "--bogus"]), Ok(Command::Version));
        assert_eq!(parse_strs(&["-V"]), Ok(Command::Version));
    }

    #[test]
    fn bad_input_is_an_error() {
        assert_eq!(parse_strs(&["--bogus"]), Err(Error::Unexpected("--bogus".into())));
        assert_eq!(parse_strs(&["--fullscreen=yes"]), Err(Error::Unexpected("--fullscreen=yes".into())));
        assert_eq!(parse_strs(&["--theme"]), Err(Error::MissingValue("--theme")));
        assert_eq!(parse_strs(&["--theme", "blue"]), Err(Error::InvalidTheme("blue".into())));
    }
}
