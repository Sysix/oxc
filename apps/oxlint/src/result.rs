use std::{
    path::PathBuf,
    process::{ExitCode, Termination},
    time::Duration,
};

#[derive(Debug)]
pub enum CliRunResult {
    None,
    InvalidOptions { message: String },
    PathNotFound { paths: Vec<PathBuf> },
    LintResult(LintResult),
    PrintConfigResult { config_file: String },
    ConfigFileInitResult { message: String },
}

/// A summary of a complete linter run.
#[derive(Debug, Default)]
pub struct LintResult {
    /// The total time it took to run the linter.
    pub duration: Duration,
    /// The number of lint rules that were run.
    pub number_of_rules: usize,
    /// The number of files that were linted.
    pub number_of_files: usize,
    /// The number of warnings that were found.
    pub number_of_warnings: usize,
    /// The number of errors that were found.
    pub number_of_errors: usize,
    /// The exit unix code for, in general 0 or 1 (from `--deny-warnings` or `--max-warnings` for example)
    pub exit_code: u8,
    /// Whether or not to print a summary of the results
    pub print_summary: bool,
}

impl Termination for CliRunResult {
    #[allow(clippy::print_stdout, clippy::print_stderr)]
    fn report(self) -> ExitCode {
        match self {
            Self::None => ExitCode::from(0),
            Self::InvalidOptions { message } => {
                println!("Invalid Options: {message}");
                ExitCode::from(1)
            }
            Self::PathNotFound { paths } => {
                println!("Path {paths:?} does not exist.");
                ExitCode::from(1)
            }
            Self::LintResult(LintResult {
                duration,
                number_of_rules,
                number_of_files,
                number_of_warnings: _, // ToDo: only for tests, make snapshots
                number_of_errors: _,   // ToDo: only for tests, make snapshots
                exit_code,
                print_summary,
            }) => {
                if print_summary {
                    let threads = rayon::current_num_threads();
                    let time = Self::get_execution_time(&duration);
                    let s = if number_of_files == 1 { "" } else { "s" };

                    println!(
                        "Finished in {time} on {number_of_files} file{s} with {number_of_rules} rules using {threads} threads."
                    );
                }
                ExitCode::from(exit_code)
            }
            Self::PrintConfigResult { config_file } => {
                println!("{config_file}");
                ExitCode::from(0)
            }
            Self::ConfigFileInitResult { message } => {
                println!("{message}");
                ExitCode::from(0)
            }
        }
    }
}

impl CliRunResult {
    fn get_execution_time(duration: &Duration) -> String {
        let ms = duration.as_millis();
        if ms < 1000 {
            format!("{ms}ms")
        } else {
            format!("{:.1}s", duration.as_secs_f64())
        }
    }
}
