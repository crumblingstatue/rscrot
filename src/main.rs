use getopts::Options;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn print_usage(program: &str, opts: &Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

fn save_screenshot(path: &Path, select: bool) -> Result<(), String> {
    let mut maim = Command::new("maim");
    if select {
        maim.arg("-s");
    }
    maim.arg(path);
    let status = match maim.status() {
        Ok(status) => status,
        Err(e) => return Err(e.to_string()),
    };
    if !status.success() {
        return Err(format!("maim failed. Exit status: {}", status));
    }
    Ok(())
}

enum Choice {
    SaveAs(PathBuf),
    OpenWith(String),
    CopyToClipboard,
}

fn get_save_filename_from_zenity() -> Result<PathBuf, String> {
    let mut zenity = Command::new("zenity");
    zenity.arg("--file-selection").arg("--save");
    let output = match zenity.output() {
        Ok(output) => output,
        Err(e) => return Err(e.to_string()),
    };
    if !output.status.success() {
        return Err(format!("zenity failed. Exit status: {}", output.status));
    }
    Ok(PathBuf::from(&String::from_utf8(output.stdout).unwrap()))
}

fn get_user_choice_from_menu(viewers: &[String]) -> Result<Choice, String> {
    let mut zenity = Command::new("zenity");
    zenity
        .arg("--list")
        .arg("--title")
        .arg("Choose Action")
        .arg("--column")
        .arg("Action");
    zenity.arg("Copy to clipboard");
    zenity.arg("Save as...");
    for viewer in viewers {
        zenity.arg(&format!("Open with {}", viewer));
    }
    let output = match zenity.output() {
        Ok(output) => output,
        Err(e) => return Err(e.to_string()),
    };
    if !output.status.success() {
        return Err(format!("zenity failed. Exit status: {}", output.status));
    }
    match &output.stdout[..] {
        b"Save as...\n" => Ok(Choice::SaveAs(get_save_filename_from_zenity()?)),
        b"Copy to clipboard\n" => Ok(Choice::CopyToClipboard),
        other => {
            for viewer in viewers {
                if other == format!("Open with {}\n", viewer).as_bytes() {
                    return Ok(Choice::OpenWith(viewer.clone()));
                }
            }
            Err(format!(
                "Zenity returned unknown result {:?}",
                String::from_utf8_lossy(other)
            ))
        }
    }
}

fn open_with(viewer: String, path: &Path) -> Result<(), String> {
    let mut cmd = Command::new(viewer);
    cmd.arg(path);
    match cmd.spawn() {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

fn copy_to_clipboard(data: &[u8], target: &str) -> Result<(), String> {
    use std::io::Write;

    let mut xclip = match Command::new("xclip")
        .arg("-selection")
        .arg("clipboard")
        .arg("-target")
        .arg(target)
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return Err(e.to_string()),
    };
    {
        let stdin = match xclip.stdin {
            Some(ref mut stdin) => stdin,
            None => return Err("Child had no stdin".into()),
        };
        if let Err(e) = stdin.write_all(data) {
            return Err(e.to_string());
        }
    }
    match xclip.wait() {
        Ok(status) => {
            if !status.success() {
                Err(format!("xclip failed. Exit status: {}", status))
            } else {
                Ok(())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

fn main() {
    let mut args = env::args();
    let program = args.next().unwrap();

    let mut opts = Options::new();
    opts.optflag("s", "select", "Let the user select the area to capture");
    opts.optmulti(
        "",
        "viewer",
        "Allow viewing the image with an image viewer.",
        "IMAGE_VIEWER",
    );
    opts.optopt(
        "t",
        "timer",
        "Sleep n seconds before taking the screenshot",
        "SECONDS",
    );
    opts.optflag("h", "help", "print this help menu");
    let matches = match opts.parse(args) {
        Ok(m) => m,
        Err(f) => panic!("{}", f.to_string()),
    };
    if matches.opt_present("h") {
        print_usage(&program, &opts);
        return;
    }
    let viewers = matches.opt_strs("viewer");
    let file_path = env::temp_dir().join("rscrot_screenshot.png");
    let select = matches.opt_present("s");
    if let Some(sleep_timer) = matches.opt_str("timer") {
        let seconds = sleep_timer
            .parse()
            .expect("Timer value needs to be numeric");
        std::thread::sleep(std::time::Duration::from_secs(seconds));
    }
    save_screenshot(&file_path, select).unwrap();
    match get_user_choice_from_menu(&viewers).unwrap() {
        Choice::SaveAs(path) => {
            std::fs::copy(&file_path, path.to_str().unwrap().trim())
                .unwrap_or_else(|e| panic!("{}", e));
        }
        Choice::OpenWith(viewer) => open_with(viewer, &file_path).unwrap(),
        Choice::CopyToClipboard => {
            let image = std::fs::read(file_path).unwrap();
            copy_to_clipboard(&image, "image/png").unwrap();
        }
    }
}
