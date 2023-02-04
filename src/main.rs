#[cfg(target_os = "linux")]
use chrono::Local;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use clap::Parser;
use std::{io::Read, net::TcpStream, thread, time::Duration};

const NIST_TIME_SERVER: &str = "time.nist.gov:13";

#[derive(Parser)]
struct Args {
    #[arg(short = 'i', long = "interval", default_value = "60")]
    interval: u64,
}

#[cfg(target_os = "linux")]
fn set_system_time(datetime: DateTime<Local>) -> Result<i32, String> {
    use libc::{settimeofday, time_t, timeval};

    let timestamp = datetime.timestamp();
    let tv = timeval {
        tv_sec: timestamp as time_t,
        tv_usec: 0,
    };
    let tz = std::ptr::null();

    let result = unsafe { settimeofday(&tv as *const timeval, tz) };

    match result {
        0 => Ok(result),
        _ => Err("Error setting system time".into()),
    }
}

#[cfg(target_os = "windows")]
fn set_system_time(datetime: DateTime<Utc>) -> Result<i32, String> {
    use chrono::{Datelike, Timelike};
    use winapi::{
        shared::minwindef::{FALSE, WORD},
        um::{minwinbase::SYSTEMTIME, sysinfoapi::SetSystemTime},
    };

    let system_time = SYSTEMTIME {
        wYear: datetime.year() as WORD,
        wMonth: datetime.month() as WORD,
        wDay: datetime.day() as WORD,
        wHour: datetime.hour() as WORD,
        wMinute: datetime.minute() as WORD,
        wSecond: datetime.second() as WORD,
        wMilliseconds: datetime.timestamp_subsec_millis() as WORD,
        wDayOfWeek: 0,
    };

    let result = unsafe { SetSystemTime(&system_time) };
    match result {
        FALSE => Err("Error setting system time".into()),
        _ => Ok(0),
    }
}

fn get_nist_server_time() -> Result<String, std::io::Error> {
    let mut stream = TcpStream::connect(NIST_TIME_SERVER)?;
    let mut buffer = [0u8; 256];
    let bytes_read = stream.read(&mut buffer)?;

    let time_string = String::from_utf8_lossy(&buffer[..bytes_read])
        .trim()
        .to_string();

    Ok(time_string)
}

fn parse_nist_response(response: &str) -> DateTime<Utc> {
    let fields: Vec<&str> = response.split_whitespace().collect();
    let date = fields[1];
    let time = fields[2];
    let year = date[0..2].parse::<i32>().unwrap() + 2000;
    let month = date[3..5].parse::<u32>().unwrap();
    let day = date[6..8].parse::<u32>().unwrap();
    let hour = time[0..2].parse::<u32>().unwrap();
    let minute = time[3..5].parse::<u32>().unwrap();
    let second = time[6..8].parse::<u32>().unwrap();
    let milisseconds: f64 = fields[6].parse::<f64>().unwrap();
    let naive = NaiveDateTime::new(
        NaiveDate::from_ymd_opt(year, month, day).unwrap(),
        NaiveTime::from_hms_opt(hour, minute, second).unwrap(),
    );
    let datetime = parse_datetime(naive);
    let datetime = datetime + chrono::Duration::milliseconds(milisseconds as i64);
    datetime
}

fn parse_datetime(naive: NaiveDateTime) -> DateTime<Utc> {
    let naive_date = naive.date();
    let naive_time = naive.time();
    let datetime = Utc.from_utc_datetime(&NaiveDateTime::new(naive_date, naive_time));
    datetime
}

#[cfg(target_os = "windows")]
fn sync_with_nist_server() -> Result<DateTime<Utc>, String> {
    let time_string = get_nist_server_time().unwrap();
    let time_tm = parse_nist_response(&time_string);
    match set_system_time(time_tm) {
        Ok(_) => Ok(time_tm),
        Err(_e) => {
            return Err("Error setting system time, check your permissions.".into());
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn sync_with_nist_server() -> Result<DateTime<Utc>, String> {
    let time_string = get_nist_server_time().unwrap();
    let time_tm = parse_nist_response(&time_string);
    let local: DateTime<Local> = Local.from_utc_datetime(&time_tm.naive_utc());
    match set_system_time(local) {
        Ok(_) => Ok(time_tm),
        Err(_e) => {
            return Err("Error setting system time, check your permissions.".into());
        }
    }
}

#[cfg(target_os = "windows")]
fn main() -> windows_service::Result<()> {
    use std::{
        ffi::OsString,
        sync::{mpsc, Arc, Condvar, Mutex},
    };

    use windows_service::{
        define_windows_service,
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };

    const SERVICE_NAME: &str = "NISTTimeSync";
    const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

    define_windows_service!(ffi_service_main, my_service_main);

    fn run() -> windows_service::Result<()> {
        service_dispatcher::start(SERVICE_NAME, ffi_service_main)
    }

    fn my_service_main(_arguments: Vec<OsString>) {
        if let Err(_e) = run_service() {
            // Handle error
        }
    }

    fn run_service() -> windows_service::Result<()> {
        let (shutdown_tx, shutdown_rx) = mpsc::channel();

        let event_handler = move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                ServiceControl::Stop => {
                    shutdown_tx.send(()).unwrap();
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        };

        let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        let args = Args::parse();

        let pair = Arc::new((Mutex::new(false), Condvar::new()));
        let pair2 = pair.clone();
        let handle = thread::spawn(move || {
            let &(ref lock, ref cvar) = &*pair2;
            let mut run = lock.lock().unwrap();
            while !*run {
                run = cvar.wait(run).unwrap();
            }
        });
        match args.interval {
            1.. => {
                println!(
                    "Syncing system time with NIST server every {} {}",
                    args.interval,
                    match args.interval {
                        1 => "minute",
                        _ => "minutes",
                    }
                );
                loop {
                    let time = sync_with_nist_server();
                    match time {
                        Ok(time) => {
                            println!("System time synced with NIST server: {}", time);
                            // this is blocking windows service from receiving stop command
                            thread::sleep(Duration::from_secs(args.interval * 60));
                            let &(ref lock, ref cvar) = &*pair;
                            let mut run = lock.lock().unwrap();
                            *run = true;
                            cvar.notify_one();
                            handle.join().unwrap();
                            match shutdown_rx.recv_timeout(Duration::from_secs(1)) {
                                // Break the loop either upon stop or channel disconnect
                                Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                                // Continue work if no events were received within the timeout
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                            };
                            break;
                        }
                        Err(e) => {
                            println!("Error: {}", e);
                            break;
                        }
                    }
                }
            }
            _ => {
                println!("Interval must be higher than 0");
            }
        }

        status_handle.set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })?;

        Ok(())
    }

    run()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    let args = Args::parse();
    match args.interval {
        1.. => {
            println!(
                "Syncing system time with NIST server every {} {}",
                args.interval,
                match args.interval {
                    1 => "minute",
                    _ => "minutes",
                }
            );
            loop {
                let time = sync_with_nist_server();
                match time {
                    Ok(time) => {
                        let local: DateTime<Local> = Local.from_utc_datetime(&time.naive_utc());
                        println!("System time synced with NIST server: {}", local);
                        thread::sleep(Duration::from_secs(args.interval * 60));
                    }
                    Err(e) => {
                        println!("Error syncing system time: {}", e);
                        break;
                    }
                }
            }
        }
        _ => {
            println!("Interval must be higher than 0");
            return;
        }
    }
}
