#![feature(path, io)]

extern crate getopts;
extern crate libnotify;

use getopts::Options;
use std::env;
use std::process::{Command, Stdio};
use std::path::{Path, PathBuf};

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
        Err(e) => return Err(e.to_string())
    };
    if !status.success() {
        return Err(format!("scrot failed. Exit status: {}", status));
    }
    Ok(())
}

enum Choice {
    Upload,
    SaveAs(PathBuf),
    OpenInFeh
}

fn get_save_filename_from_zenity() -> Result<PathBuf, String> {
    let mut zenity = Command::new("zenity");
    zenity.arg("--file-selection").arg("--save");
    let output = match zenity.output() {
        Ok(output) => output,
        Err(e) => return Err(e.to_string())
    };
    if !output.status.success() {
        return Err(format!("zenity failed. Exit status: {}", output.status));
    }
    Ok(PathBuf::new(&String::from_utf8(output.stdout).unwrap()))
}

fn get_user_choice_from_menu() -> Result<Choice, String> {
    let mut zenity = Command::new("zenity");
    zenity
     .arg("--list")
     .arg("--title").arg("Choose Action")
     .arg("--column").arg("id")
     .arg("--column").arg("Name")
     .arg("1").arg("Upload to imgur.com")
     .arg("2").arg("Save as...")
     .arg("3").arg("Open in feh");
    let output = match zenity.output() {
        Ok(output) => output,
        Err(e) => return Err(e.to_string())
    };
    if !output.status.success() {
        return Err(format!("zenity failed. Exit status: {}", output.status));
    }
    match output.stdout[0] {
        b'1' => Ok(Choice::Upload),
        b'2' => Ok(Choice::SaveAs(try!(get_save_filename_from_zenity()))),
        b'3' => Ok(Choice::OpenInFeh),
        id => Err(format!("Zenity returned unknown result {}", id))
    }
}

fn upload_to_imgur(path: &Path) -> Result<String, String> {
    let mut imgur = Command::new("imgur");
    imgur.arg(path);
    let output = match imgur.output() {
        Ok(output) => output,
        Err(e) => return Err(e.to_string())
    };
    if !output.status.success() {
        return Err(format!("imgur failed. Exit status: {}", output.status));
    }
    match String::from_utf8(output.stdout) {
        Ok(url) => Ok(url),
        Err(e) => return Err(e.to_string())
    }
}

fn save_as(orig_path: &Path, new_path: &Path) -> Result<(), String> {
    unimplemented!()
}

fn open_in_feh(path: &Path) -> Result<(), String> {
    unimplemented!()
}

fn copy_to_clipboard(string: &str) -> Result<(), String> {
    use std::io::Write;

    let mut xclip = match Command::new("xclip")
                          .arg("-selection").arg("clipboard")
                          .stdin(Stdio::piped())
                          .spawn() {
        Ok(child) => child,
        Err(e) => return Err(e.to_string())
    };
    {
        let stdin = match xclip.stdin {
            Some(ref mut stdin) => stdin,
            None => return Err("Child had no stdin".to_string())
        };
        if let Err(e) = stdin.write_all(string.as_bytes()) {
            return Err(e.to_string())
        }
    }
    match xclip.wait() {
        Ok(status) => {
            if !status.success() {
                return Err(format!("xclip failed. Exit status: {}", status));
            } else {
                Ok(())
            }
        },
        Err(e) => Err(e.to_string())
    }
}

fn main() {
    let mut args = env::args();
    let program = args.next().unwrap();

    let mut opts = Options::new();
    opts.optflag("s", "select",
                 "Let the user select the area to capture");
    opts.optflag("h", "help", "print this help menu");
    let matches = match opts.parse(args) {
        Ok(m) => { m }
        Err(f) => { panic!(f.to_string()) }
    };
    if matches.opt_present("h") {
        print_usage(&program, opts);
        return;
    }
    let file_path = env::temp_dir().join("rscrot_screenshot.png");
    let select = matches.opt_present("s");
    save_screenshot(&file_path, select).unwrap();
    match get_user_choice_from_menu().unwrap() {
        Choice::Upload => {
            let url = upload_to_imgur(&file_path).unwrap();
            copy_to_clipboard(&url).unwrap();
            let notify = libnotify::Context::new("rscrot").unwrap();
            let body = format!("Uploaded to {}", url);
            let msg = notify.new_notification("Success:", Some(&body), None).unwrap();
            msg.show().unwrap();
        }
        Choice::SaveAs(path) => save_as(&file_path, &path).unwrap(),
        Choice::OpenInFeh => open_in_feh(&file_path).unwrap()
    }
}
