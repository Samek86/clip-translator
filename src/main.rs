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
        eprintln!("clip-translator는 Windows 10/11 전용 프로그램입니다.");
        std::process::exit(1);
    }
}
