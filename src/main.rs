extern crate getopts;
extern crate libnotify;
extern crate imgur;

use getopts::Options;
use std::env;
use std::process::{Command, Stdio};
use std::path::{Path, PathBuf};
use std::error::Error;
use std::fs::File;

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

fn save_screenshot(path: &Path, select: bool) -> Result<(), String> {
    let mut scrot = Command::new("scrot");
    if select {
        scrot.arg("-s");
    }
    scrot.arg(path);
    let status = match scrot.status() {
        Ok(status) => status,
        Err(e) => return Err(e.to_string()),
    };
    if !status.success() {
        return Err(format!("scrot failed. Exit status: {}", status));
    }
    Ok(())
}

enum Choice {
    Upload,
    SaveAs(PathBuf),
    OpenWith(String),
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

fn get_user_choice_from_menu(imgur: bool, viewer: Option<String>) -> Result<Choice, String> {
    let mut zenity = Command::new("zenity");
    zenity.arg("--list")
          .arg("--title")
          .arg("Choose Action")
          .arg("--column")
          .arg("Action");
    if imgur {
        zenity.arg("Upload to imgur.com");
    }
    zenity.arg("Save as...");
    if let Some(ref viewer) = viewer {
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
        b"Save as...\n" => Ok(Choice::SaveAs(try!(get_save_filename_from_zenity()))),
        other => {
            if let Some(viewer) = viewer {
                if other == format!("Open with {}\n", viewer).as_bytes() {
                    return Ok(Choice::OpenWith(viewer));
                }
            }
            Err(format!("Zenity returned unknown result {:?}",
                        String::from_utf8_lossy(other)))
        }
    }
}

fn upload_to_imgur(path: &Path, client_id: String) -> Result<imgur::UploadInfo, Box<Error>> {
    use std::io::Read;
    let mut file = try!(File::open(path));
    let mut data = Vec::new();
    try!(file.read_to_end(&mut data));
    let handle = imgur::Handle::new(client_id);
    Ok(try!(handle.upload(&data)))
}

fn open_with(viewer: String, path: &Path) -> Result<(), String> {
    let mut cmd = Command::new(viewer);
    cmd.arg(path);
    match cmd.spawn() {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

fn copy_to_clipboard(string: &str) -> Result<(), String> {
    use std::io::Write;

    let mut xclip = match Command::new("xclip")
                              .arg("-selection")
                              .arg("clipboard")
                              .stdin(Stdio::piped())
                              .spawn() {
        Ok(child) => child,
        Err(e) => return Err(e.to_string()),
    };
    {
        let stdin = match xclip.stdin {
            Some(ref mut stdin) => stdin,
            None => return Err("Child had no stdin".into()),
        };
        if let Err(e) = stdin.write_all(string.as_bytes()) {
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
    opts.optopt("",
                "imgur",
                "Allow uploading to imgur. Needs client id.",
                "CLIENT_ID");
    opts.optopt("",
                "viewer",
                "Allow viewing the image with an image viewer.",
                "IMAGE_VIEWER");
    opts.optflag("h", "help", "print this help menu");
    let matches = match opts.parse(args) {
        Ok(m) => m,
        Err(f) => panic!(f.to_string()),
    };
    if matches.opt_present("h") {
        print_usage(&program, opts);
        return;
    }
    let client_id = matches.opt_str("imgur");
    let opt_viewer = matches.opt_str("viewer");
    let file_path = env::temp_dir().join("rscrot_screenshot.png");
    let select = matches.opt_present("s");
    save_screenshot(&file_path, select).unwrap();
    match get_user_choice_from_menu(client_id.is_some(), opt_viewer).unwrap() {
        Choice::Upload => {
            let notify = libnotify::Context::new("rscrot").unwrap();
            match upload_to_imgur(&file_path, client_id.unwrap()) {
                Ok(info) => {
                    match info.link() {
                        Some(url) => {
                            copy_to_clipboard(url).unwrap();
                            let body = format!("Uploaded to {}", url);
                            let msg = notify.new_notification("Success:", Some(&body), None)
                                            .unwrap();
                            msg.show().unwrap();
                        }
                        None => {
                            let msg = notify.new_notification("Wtf, no link?", None, None)
                                            .unwrap();
                            msg.show().unwrap();
                        }
                    }
                }
                Err(e) => {
                    let msg = notify.new_notification(&format!("Error: {}", e), None, None)
                                    .unwrap();
                    msg.show().unwrap();
                }
            }
        }
        Choice::SaveAs(path) => {
            std::fs::copy(&file_path, path.to_str().unwrap().trim())
                .unwrap_or_else(|e| panic!("{}", e));
        }
        Choice::OpenWith(viewer) => open_with(viewer, &file_path).unwrap(),
    }
}
