use std::thread;
use std::time::{Duration, SystemTime};

use clap::{Command, Arg, ArgAction};

use std::process::Command as ProcessCommand;

use crate::spindown_daemon::{DeviceInfo, get_device_info};
use crate::spindown_daemon::ata::{do_standby, PowerState};

mod spindown_daemon;

fn main() {
    let greater_than_zero_value_parser = |val: &str| {
        match val.parse::<u64>() {
            Ok(num) => {
                if num < 1 {
                    return Err(String::from("value must be greater than 0"));
                }
                Ok(num)
            }
            Err(e) => Err(e.to_string())
        }
    };

    let matches = Command::new("spindown-daemon")
        .version("1.0")
        .author("x1125 <git@1125.io>")
        .about("Spin-down hard disks without relying on the firmware")
        .arg(Arg::new("check-timeout")
            .short('i')
            .help("Check interval in seconds (default: 60)")
            .default_value("60")
            .value_parser(greater_than_zero_value_parser))
        .arg(Arg::new("iops-tolerance")
            .short('t')
            .help("Tolerance for read/write IO operations (default: 1)")
            .long_help(
                "Put device to sleep, even if this amount of IOPS have been read/written; \
                Checking the power state adds one read, so using 0 would prevent sleep \
                completely and thus is not allowed."
            )
            .default_value("1")
            .value_parser(greater_than_zero_value_parser))
        .arg(Arg::new("suspend")
            .long("suspend")
            .help("Suspend system after all drives are sleeping")
            .action(ArgAction::SetTrue))
        .arg(Arg::new("suspend-timeout")
            .long("suspend-timeout")
            .help("Wait n-seconds before system suspend after all drives are sleeping")
            .default_value("60")
            .value_parser(greater_than_zero_value_parser))
        .arg(Arg::new("suspend-check-script")
            .long("suspend-check-script")
            .help("Path of external script to block the system suspension")
            .long_help("Exit code 0 allows suspend; every other code will block it"))
        .arg(Arg::new("debug")
            .short('d')
            .help("Enable debug output")
            .action(ArgAction::SetTrue))
        .arg(Arg::new("DEVICE:TIMEOUT")
            .long_help(
                "Device-names and timeout in seconds
Example: sda1:3600 md127:600")
            .required(true)
            .num_args(1..)
            .value_parser(|val: &str| -> Result<String, &str> {
                let (device_name_str, device_timeout_str) = if let Some((a, b)) = val.split_once(':') {
                    (a, b)
                } else {
                    return Err("invalid amount of elements");
                };

                let device_name = String::from(device_name_str);
                if !device_name.starts_with("sd") || !device_name.ends_with(|v: char| {
                    // allow a-z only
                    let unicode = v as u32;
                    unicode >= 97 && unicode <= 122
                }) {
                    return Err("device name must have format `sd[a-z]`");
                }
                match device_timeout_str.parse::<u64>() {
                    Ok(device_timeout) => {
                        if device_timeout < 1 {
                            return Err("device timeout must be greater than 0");
                        }
                    }
                    Err(_) => return Err("device timeout must be a number")
                }
                Ok(String::from(val))
            })
        )
        .get_matches();

    if matches.get_flag("debug") {
        stderrlog::new().
            verbosity(log::LevelFilter::Debug as usize).
            module(module_path!()).
            init().unwrap();
    }

    let mut devices: Vec<Box<DeviceInfo>> = vec![];
    for item in matches.get_many::<String>("DEVICE:TIMEOUT").unwrap() {
        let (device_name, device_timeout_str) = item.split_once(':').unwrap();
        let device_timeout: u64 = device_timeout_str.parse().unwrap();

        match get_device_info(&device_name.to_owned().to_string()) {
            Ok(mut dev_info) => {
                dev_info.timeout = device_timeout;
                log::debug!("added {:?}", dev_info);
                devices.push(Box::new(dev_info));
            }
            Err(e) => println!("unable to get device information for {}: {}", e.filepath, e.message)
        }
    }

    if devices.len() < 1 {
        println!("no devices to watch. exiting...");
        return;
    }

    let check_interval: u64 = *matches.get_one("check-timeout").unwrap();
    let iops_tolerance: u64 = *matches.get_one("iops-tolerance").unwrap();
    log::debug!("iops_tolerance: {:?}", iops_tolerance);

    let suspend: bool = matches.get_flag("suspend");
    let suspend_timeout: u64 = *matches.get_one("suspend-timeout").unwrap();
    let suspend_check_script: Option<&String> = matches.get_one::<String>("suspend-check-script");

    loop {
        log::debug!("sleeping for {} seconds", check_interval);
        thread::sleep(Duration::from_secs(check_interval));

        let mut disks_running: bool = false;
        let mut latest_update: SystemTime = SystemTime::UNIX_EPOCH;

        for cache in devices.iter_mut() {
            match get_device_info(&cache.name) {
                Ok(current) => {
                    log::debug!("cache {:?}", cache);
                    log::debug!("current {:?}", current);

                    cache.power_state = current.power_state;
                    let mut no_iops = false;

                    if cache.last_read_iops == current.last_read_iops &&
                        cache.last_write_iops == current.last_write_iops {
                        no_iops = true;
                        log::debug!("device {:?} did not change", current.name)
                    } else {
                        if (cache.last_read_iops + iops_tolerance) >= current.last_read_iops &&
                            (cache.last_write_iops + iops_tolerance) >= current.last_write_iops {
                            no_iops = true;
                            log::debug!("device {:?} is within tolerance", current.name)
                        }

                        cache.last_read_iops = current.last_read_iops;
                        cache.last_write_iops = current.last_write_iops;

                        if !no_iops {
                            cache.last_update = current.last_update;
                        }
                    }

                    if no_iops &&
                        cache.last_update.elapsed().unwrap().as_secs() > cache.timeout &&
                        cache.power_state != PowerState::Standby {
                        log::debug!("issuing standby for {}", cache.name);
                        match do_standby(&cache.name) {
                            Ok(()) => println!("issued standby for {}", cache.name),
                            Err(e) => println!("unable to issue standby for {}: {}",
                                               e.filepath, e.message)
                        }
                        cache.last_update = current.last_update;
                    }

                    if cache.power_state != PowerState::Standby {
                        disks_running = true;
                    }
                    if cache.last_update > latest_update {
                        latest_update = cache.last_update;
                    }

                    log::debug!("updated cache {:?}", cache);
                }
                Err(e) => println!("unable to get device information for {}: {}", e.filepath, e.message)
            }
        }

        if suspend {
            log::debug!("checking system suspend");
            if disks_running {
                log::debug!("disk(s) still running");
                continue;
            }

            if latest_update.elapsed().unwrap().as_secs() < suspend_timeout {
                log::debug!("suspend timeout not met");
                continue;
            }

            match suspend_check_script {
                Some(script) => {
                    log::debug!("executing check script");
                    let cmd = ProcessCommand::new("bash")
                        .arg(script)
                        .output()
                        .expect("failed to execute process");
                    if cmd.status.code().unwrap() != 0 {
                        log::debug!("script exited with non zero code ({})", cmd.status.code().unwrap());
                        continue;
                    }
                }
                None => {}
            }

            log::debug!("suspending system...");
            ProcessCommand::new("/usr/bin/systemctl")
                .arg("suspend")
                .output()
                .expect("failed to execute process");
        }
    }
}