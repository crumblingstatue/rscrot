extern crate getopts;
extern crate libnotify;
extern crate imgur;
extern crate clipboard;

use getopts::Options;
use std::env;
use std::process::Command;
use std::path::{Path, PathBuf};
use std::error::Error;
use std::fs::File;
use std::time::{Instant, Duration};
use clipboard::ClipboardContext;

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

fn get_user_choice_from_menu(imgur: bool, viewers: &[String]) -> Result<Choice, String> {
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
        b"Save as...\n" => Ok(Choice::SaveAs(try!(get_save_filename_from_zenity()))),
        other => {
            for viewer in viewers {
                if other == format!("Open with {}\n", viewer).as_bytes() {
                    return Ok(Choice::OpenWith(viewer.clone()));
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

fn main() {
    let mut args = env::args();
    let program = args.next().unwrap();

    let mut opts = Options::new();
    opts.optflag("s", "select", "Let the user select the area to capture");
    opts.optopt("",
                "imgur",
                "Allow uploading to imgur. Needs client id.",
                "CLIENT_ID");
    opts.optmulti("",
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
    let viewers = matches.opt_strs("viewer");
    let file_path = env::temp_dir().join("rscrot_screenshot.png");
    let select = matches.opt_present("s");
    save_screenshot(&file_path, select).unwrap();
    match get_user_choice_from_menu(client_id.is_some(), &viewers).unwrap() {
        Choice::Upload => {
            let notify = libnotify::Context::new("rscrot").unwrap();
            match upload_to_imgur(&file_path, client_id.unwrap()) {
                Ok(info) => {
                    match info.link() {
                        Some(url) => {
                            // Copy url to clipboard
                            {
                                let mut ctx = ClipboardContext::new().unwrap();
                                ctx.set_contents(url.to_owned()).unwrap();
                            }
                            let body = format!("Uploaded to {}", url);
                            let msg = notify.new_notification("Success:", Some(&body), None)
                                .unwrap();
                            msg.show().unwrap();
                            // X11 clipboard manegement sucks.
                            // Wait for a while, either for user to paste link, or preferrably,
                            // the user's clipboard manager to pick the contents up.
                            let wait_for = Duration::from_secs(20);
                            let wait_start = Instant::now();

                            while wait_start.elapsed() < wait_for {
                                // Wait in a semi-busy state, since straight-up sleeping
                                // doesn't seem to work? I don't know anymore.
                                std::thread::sleep(std::time::Duration::from_millis(50));
                            }
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
