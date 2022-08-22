extern crate getopts;
extern crate imgur;
extern crate notify_rust;

use getopts::Options;
use std::env;
use std::error::Error;
use std::fs::File;
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
    Upload,
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

fn get_user_choice_from_menu(imgur: bool, viewers: &[String]) -> Result<Choice, String> {
    let mut zenity = Command::new("zenity");
    zenity
        .arg("--list")
        .arg("--title")
        .arg("Choose Action")
        .arg("--column")
        .arg("Action");
    if imgur {
        zenity.arg("Upload to imgur.com");
    }
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
        b"Upload to imgur.com\n" => Ok(Choice::Upload),
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

fn upload_to_imgur(path: &Path, client_id: String) -> Result<imgur::UploadInfo, Box<dyn Error>> {
    use std::io::Read;
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    let handle = imgur::Handle::new(client_id);
    Ok(handle.upload(&data)?)
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
    opts.optopt(
        "",
        "imgur",
        "Allow uploading to imgur. Needs client id.",
        "CLIENT_ID",
    );
    opts.optmulti(
        "",
        "viewer",
        "Allow viewing the image with an image viewer.",
        "IMAGE_VIEWER",
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
    let client_id = matches.opt_str("imgur");
    let viewers = matches.opt_strs("viewer");
    let file_path = env::temp_dir().join("rscrot_screenshot.png");
    let select = matches.opt_present("s");
    save_screenshot(&file_path, select).unwrap();
    match get_user_choice_from_menu(client_id.is_some(), &viewers).unwrap() {
        Choice::Upload => {
            use notify_rust::Notification;
            match upload_to_imgur(&file_path, client_id.unwrap()) {
                Ok(info) => match info.link() {
                    Some(url) => {
                        copy_to_clipboard(url.as_bytes(), "text/plain").unwrap();
                        let body = format!("Uploaded to {}", url);
                        Notification::new()
                            .summary("Success:")
                            .body(&body)
                            .show()
                            .unwrap();
                    }
                    None => {
                        Notification::new().summary("Wtf, no link?").show().unwrap();
                    }
                },
                Err(e) => {
                    Notification::new()
                        .summary(&format!("Error: {}", e))
                        .show()
                        .unwrap();
                }
            }
        }
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
