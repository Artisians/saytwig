//! Saytwig native application entry point.
fn main() -> std::process::ExitCode {
    match dictation_app::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
