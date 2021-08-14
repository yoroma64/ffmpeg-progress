use regex::bytes::Regex;
use std::env::args;
use std::io::{stdout, BufRead, BufReader, ErrorKind, Write};
use std::process::{exit, Command, Stdio};
use std::time::SystemTime;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn progress(bytes: &[u8], regex: &Regex, total_sec: &mut u32) {
    if let Some(result) = regex.captures(bytes) {
        let mut mult = 3600;
        for i in 1..4 {
            *total_sec += mult
                * std::str::from_utf8(&result[i])
                    .unwrap()
                    .parse::<u32>()
                    .unwrap();
            mult /= 60;
        }
    }
}

fn match_bytes(bytes: &[u8], regex: &Regex, float: &mut f32) {
    if let Some(result) = regex.captures(bytes) {
        *float = std::str::from_utf8(&result[1])
            .unwrap()
            .parse::<f32>()
            .unwrap();
    }
}

fn human_readable(bytes: f32, string: &mut String) {
    if bytes > 1_000_000.0 {
        *string = format!("{:.1}GB", bytes / 1_000_000.0);
    } else if bytes > 1000.0 {
        *string = format!("{:.1}MB", bytes / 1000.0);
    } else {
        *string = format!("{:.1}KB", bytes);
    }
}

fn secs_to_time(secs: f32, min_sec: &mut f32, string: &mut String) {
    if secs > 3600.0 {
        *min_sec = secs % 3600.0 * 60.0;
        if *min_sec > 59.0 {
            *min_sec = 59.0;
        }
        *string = format!("{0:.0}h {1:.0}m", secs / 3600.0, min_sec);
    } else if secs > 60.0 {
        *min_sec = secs % 60.0;
        if *min_sec > 59.0 {
            *min_sec = 59.0;
        }
        *string = format!("{0:.0}m {1:.0}s", secs / 60.0, min_sec);
    } else {
        *string = format!("{:.0}s", secs);
    }
}

fn backspace(string: &str) {
    print!("{}", "\u{8} \u{8}".repeat(string.len()));
    stdout().flush().unwrap();
}

fn progress_bar(output: &mut String, percent: f32, mult: &mut usize, bar_width: usize) {
    *mult = (percent * bar_width as f32 / 100.0) as usize;
    *output = format!("[{0}{1}]", "#".repeat(*mult), ".".repeat(bar_width - *mult));
}

fn ffmpeg(arg: &[String], stats: bool, bar_width: usize) {
    let duration_regex = Regex::new(r"Duration: (\d{2}):(\d{2}):(\d{2})\.\d{2}").unwrap();
    let time_regex = Regex::new(r"time=(\d{2}):(\d{2}):(\d{2})\.\d{2}").unwrap();
    let speed_regex = Regex::new(r"speed=(\d+\.\d+)").unwrap();
    let cur_size_regex = Regex::new(r"size=\s*(\d+)").unwrap();

    let mut duration = 0;
    let mut time = 0;
    let mut mult = 0;
    let mut speed = 0.0;
    let mut eta = 0.0;
    let mut size = 0.0;
    let mut old_cur_size = 0.0;
    let mut percent = 0.0;
    let mut bitrate = 0.0;
    let mut cur_size = 0.0;
    let mut min_sec = 0.0;
    let mut time_elapsed = 0.0;
    let mut bitrate_str = String::new();
    let mut cur_size_str = String::new();
    let mut eta_str = String::new();
    let mut size_str = String::new();
    let mut start_time = SystemTime::now();
    let mut sys_time = start_time;
    let mut old_sys_time = start_time;
    let mut file_exists = false;

    let mut ffmpeg = &mut Command::new("ffmpeg");
    ffmpeg = ffmpeg.args(arg);

    let mut child = match ffmpeg.stderr(Stdio::piped()).spawn() {
        Ok(o) => o,
        Err(e) => match e.kind() {
            ErrorKind::NotFound => {
                println!("ffmpeg not installed or not in PATH");
                exit(1);
            }
            e => {
                println!("{:?}", e);
                exit(1);
            }
        },
    };

    let mut output: String;
    if bar_width > 0 {
        output = format!("[{}] 0%", ".".repeat(bar_width));
    } else {
        output = "0%".to_string();
    }
    print!("{}", output);
    stdout().flush().unwrap();

    let err = BufReader::new(child.stderr.take().unwrap());

    err.split(b']').for_each(|bytes| {
        if std::str::from_utf8(bytes.as_ref().unwrap())
            .unwrap()
            .contains(&"already exists. Overwrite? [y/N")
        {
            backspace(&output);
            output = "".to_string();
            file_exists = true;
            print!(
                "{}] ",
                std::str::from_utf8(bytes.as_ref().unwrap())
                    .unwrap()
                    .rsplit_once('\n')
                    .unwrap()
                    .1
            );
            stdout().flush().unwrap();
        } else {
            if file_exists {
                start_time = SystemTime::now();
                file_exists = false;
            }

            if duration == 0 {
                progress(bytes.as_ref().unwrap(), &duration_regex, &mut duration);
            }

            time = 0;
            progress(bytes.as_ref().unwrap(), &time_regex, &mut time);

            if time != 0 {
                old_sys_time = sys_time;
                sys_time = SystemTime::now();
                time_elapsed =
                    sys_time.duration_since(old_sys_time).unwrap().as_millis() as f32 / 1000.0;

                old_cur_size = cur_size;
                match_bytes(bytes.as_ref().unwrap(), &cur_size_regex, &mut cur_size);
                match_bytes(bytes.as_ref().unwrap(), &speed_regex, &mut speed);
                percent = time as f32 * 100.0 / duration as f32;
                size = 100.0 / percent * cur_size;
                bitrate = (cur_size - old_cur_size) / time_elapsed;

                eta = (duration - time) as f32 / speed;
                secs_to_time(eta, &mut min_sec, &mut eta_str);

                human_readable(size, &mut size_str);
                human_readable(bitrate, &mut bitrate_str);
                human_readable(cur_size, &mut cur_size_str);

                backspace(&output);
                if bar_width > 0 {
                    progress_bar(&mut output, percent, &mut mult, bar_width);
                }

                if stats {
                    output = format!(
                        "{0} {1:.1}%/{2} of ~{3} at {4}/s ETA {5}",
                        output, percent, cur_size_str, size_str, bitrate_str, eta_str
                    );
                } else {
                    output = format!("{0} {1:.1}%", output, percent);
                }

                print!("{}", output);
                stdout().flush().unwrap();
            }
        }
    });

    let status = child.wait().unwrap();

    backspace(&output);

    let end_time = SystemTime::now();
    time_elapsed = end_time.duration_since(start_time).unwrap().as_secs() as f32;
    let mut time_elapsed_str = String::new();
    secs_to_time(time_elapsed, &mut min_sec, &mut time_elapsed_str);

    bitrate = cur_size / time_elapsed;
    human_readable(bitrate, &mut bitrate_str);

    if status.success() {
        if bar_width > 0 {
            output = format!("[{}] ", "#".repeat(bar_width));
        } else {
            output = "".to_string();
        }
        if stats {
            println!(
                "{0}100% of {1} in {2} at {3}/s",
                output, cur_size_str, time_elapsed_str, bitrate_str
            );
        } else {
            println!("{}100%", output);
        }
    } else {
        println!("Process failed!");
    }
}

fn main() {
    let mut arg: Vec<String> = args().collect();

    if arg.len() > 1 {
        let invalid = "Invalid arguments!";
        if arg.len() == 2 {
            if ["-h", "--help"].contains(&arg[1].as_str()) {
                println!("usage: {} [options]\noptions:\n-h, --help       show help\n-v, --version    print version\n--no-stats       show only progress bar\n--bar-width      set progress bar width\nAll other options are passed directly to ffmpeg.", arg[0]);
                exit(0);
            } else if ["-v", "--version"].contains(&arg[1].as_str()) {
                println!("v{}", VERSION);
                exit(0);
            } else {
                println!("{}", invalid);
                exit(1);
            }
        } else {
            let mut stats = true;
            let mut bar_width = 20_usize;
            let mut indexes = Vec::<usize>::new();

            for (i, a) in arg.iter().enumerate() {
                if a == "--no-stats" {
                    stats = false;
                    indexes.push(i);
                }

                if a == "--bar-width" {
                    if i + 1 < arg.len() {
                        bar_width = if let Ok(w) = arg[i + 1].parse::<usize>() {
                            indexes.extend([i, i + 1]);
                            w
                        } else {
                            println!("{}", invalid);
                            exit(1);
                        };
                    } else {
                        println!("{}", invalid);
                    }
                }
            }
            for (i, index) in indexes.iter().enumerate() {
                arg.remove(index - i);
            }

            let arg = ["-loglevel".to_string(), "level".to_string()]
                .iter()
                .chain(&arg[1..])
                .cloned()
                .collect::<Vec<String>>();
            ffmpeg(&arg, stats, bar_width);
        }
    } else {
        println!("No arguments supplied!");
        exit(1);
    }
}
