#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    {
        if let Err(err) = clip_translator::run() {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("clip-translator runs only on Windows 10 and 11.");
        std::process::exit(1);
    }
}
