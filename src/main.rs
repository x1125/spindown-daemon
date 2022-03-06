use std::thread;
use std::time::Duration;

use clap::{App, Arg};

use crate::spindown_daemon::{DeviceInfo, get_device_info};
use crate::spindown_daemon::ata::{do_standby, PowerState};

mod spindown_daemon;

fn main() {
    let matches = App::new("spindown-daemon")
        .version("1.0")
        .author("x1125 <git@1125.io>")
        .about("Spin-down hard disks without relying on the firmware")
        .arg(Arg::new("check-timeout")
            .short('i')
            .about("Check interval in seconds (default: 60)")
            .default_value("60")
            .validator(|val| {
                match val.parse::<u64>() {
                    Ok(check_timeout) => {
                        if check_timeout < 1 {
                            return Err(String::from("interval must be greater than 0"));
                        }
                        Ok(())
                    }
                    Err(e) => Err(e.to_string())
                }
            }))
        .arg(Arg::new("iops-tolerance")
            .short('t')
            .about("Tolerance for read/write IO operations (default: 1)")
            .long_about(
                "Put device to sleep, even if this amount of IOPS have been read/written; \
                Checking the power state adds one read, so using 0 would prevent sleep \
                completely and thus is not allowed."
            )
            .default_value("1")
            .validator(|val| {
                match val.parse::<u64>() {
                    Ok(iops_tolerance) => {
                        if iops_tolerance < 1 {
                            return Err(String::from("interval must be greater than 0"));
                        }
                        Ok(())
                    }
                    Err(e) => Err(e.to_string())
                }
            }))
        .arg(Arg::new("debug")
            .short('d')
            .about("Enable debug output"))
        .arg(Arg::new("DEVICE:TIMEOUT")
            .long_about(
                "Device-names and timeout in seconds
Example: sda1:3600 md127:600")
            .required(true)
            .multiple(true)
            .validator(|val| {
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
                Ok(())
            })
        )
        .get_matches();

    if matches.is_present("debug") {
        stderrlog::new().
            verbosity(log::LevelFilter::Debug as usize).
            module(module_path!()).
            init().unwrap();
    }

    let mut devices: Vec<Box<DeviceInfo>> = vec![];
    for item in matches.values_of("DEVICE:TIMEOUT").unwrap() {
        let (device_name, device_timeout_str) = item.split_once(':').expect("asd");
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

    let check_interval: u64 = matches.value_of("check-timeout").unwrap().parse().unwrap();
    let iops_tolerance: u64 = matches.value_of("iops-tolerance").unwrap().parse().unwrap();
    loop {
        log::debug!("sleeping for {} seconds", check_interval);
        thread::sleep(Duration::from_secs(check_interval));

        for dev in devices.iter_mut() {
            match get_device_info(&dev.name) {
                Ok(dev_info) => {
                    log::debug!("current {:?}", dev);
                    log::debug!("fetched {:?}", dev_info);

                    dev.power_state = dev_info.power_state;
                    let mut no_iops = false;

                    if dev.last_read_iops != dev_info.last_read_iops ||
                        dev.last_write_iops != dev_info.last_write_iops {

                        if (dev.last_read_iops + iops_tolerance) >= dev_info.last_read_iops &&
                            (dev.last_write_iops + iops_tolerance) >= dev_info.last_write_iops {
                            no_iops = true
                        }

                        dev.last_read_iops = dev_info.last_read_iops;
                        dev.last_write_iops = dev_info.last_write_iops;

                        if !no_iops {
                            dev.last_update = dev_info.last_update;
                        }
                    }

                    if no_iops &&
                        dev.last_update.elapsed().unwrap().as_secs() > dev.timeout &&
                        dev.power_state != PowerState::Standby {
                        log::debug!("issuing standby for {}", dev.name);
                        match do_standby(&dev.name) {
                            Ok(()) => println!("issued standby for {}", dev.name),
                            Err(e) => println!("unable to issue standby for {}: {}",
                                               e.filepath, e.message)
                        }
                        dev.last_update = dev_info.last_update;
                    }
                    log::debug!("updated {:?}", dev);
                }
                Err(e) => println!("unable to get device information for {}: {}", e.filepath, e.message)
            }
        }
    }
}