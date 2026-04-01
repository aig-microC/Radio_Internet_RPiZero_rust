use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::time::sleep;

#[cfg(target_os = "linux")]
use rppal::gpio::Gpio;
#[cfg(target_os = "linux")]
use rppal::gpio::Level;
#[cfg(target_os = "linux")]
use rppal::i2c::I2c;

const GPIO_PIN_NEXT: u8 = 20;
const GPIO_PIN_PREV: u8 = 16;
const GPIO_PIN_TIMER: u8 = 12;
const GPIO_PIN_NEWS: u8 = 6;

const LCD_CLEARDISPLAY: u8 = 0x01;
const LCD_ENTRYMODESET: u8 = 0x04;
const LCD_DISPLAYCONTROL: u8 = 0x08;
const LCD_FUNCTIONSET: u8 = 0x20;
const LCD_SETDDRAMADDR: u8 = 0x80;
const LCD_DISPLAYON: u8 = 0x04;
const LCD_CURSOROFF: u8 = 0x00;
const LCD_BLINKOFF: u8 = 0x00;
const LCD_ENTRYLEFT: u8 = 0x02;
const LCD_ENTRYSHIFTDECREMENT: u8 = 0x00;
const LCD_4BITMODE: u8 = 0x00;
const LCD_2LINE: u8 = 0x08;
const LCD_BACKLIGHT: u8 = 0x08;
const LCD_ENABLE: u8 = 0x04;
const LCD_RS: u8 = 0x01;

const DEBOUNCE_MS: u64 = 50;
const LONG_PRESS_MS: u64 = 4000;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = init_lcd1602();
    
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
    
    let exe_path = env::current_exe()?;
    let exe_dir = exe_path.parent().unwrap_or_else(|| Path::new("."));
    
    let m3u_path = exe_dir.join("emisoras.m3u");
    let last_station_path = exe_dir.join("última_estación.txt");
    let news_m3u_path = exe_dir.join("noticias.m3u");
    let news_minutes_path = exe_dir.join("minutos_noticias.txt");
    
    let stations = get_all_stations_from_m3u(&m3u_path.to_string_lossy())?;
    
    if stations.is_empty() {
        return Ok(());
    }
    
    let station_names = get_station_names_from_m3u(&m3u_path.to_string_lossy())?;
    
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    
    let current_station_index = Arc::new(AtomicUsize::new(
        get_last_station_index(&last_station_path.to_string_lossy())?
    ));
    
    let mut index = current_station_index.load(Ordering::SeqCst);
    if index >= stations.len() {
        index = 0;
        current_station_index.store(index, Ordering::SeqCst);
        save_last_station_index(&last_station_path.to_string_lossy(), index)?;
    }
    
    let running = Arc::new(AtomicBool::new(true));
    let station_switch = Arc::new(AtomicBool::new(false));
    let station_direction = Arc::new(AtomicIsize::new(1));
    
    let timer_minutes = Arc::new(AtomicUsize::new(0));
    let timer_active = Arc::new(AtomicBool::new(false));
    let timer_start_time = Arc::new(AtomicUsize::new(0));
    let timer_should_poweroff = Arc::new(AtomicBool::new(false));
    
    let news_enabled = Arc::new(AtomicBool::new(false));
    let news_active = Arc::new(AtomicBool::new(false));
    let news_start_minute = Arc::new(AtomicUsize::new(0));
    let news_end_minute = Arc::new(AtomicUsize::new(5));
    let news_station = Arc::new(AtomicBool::new(false));
    let saved_station_index = Arc::new(AtomicUsize::new(0));
    
    load_news_config(&news_minutes_path.to_string_lossy(), &news_start_minute, &news_end_minute)?;
    
    let news_stations = get_all_stations_from_m3u(&news_m3u_path.to_string_lossy())?;
    let news_url = if news_stations.is_empty() {
        String::new()
    } else {
        news_stations[0].clone()
    };
    
    let r_running = running.clone();
    let r_current_index = current_station_index.clone();
    let stations_count = stations.len();
    let _r_station_direction = station_direction.clone();
    let r_timer_start = timer_start_time.clone();
    let r_running_gpio = running.clone();
    
    // Clones para el hilo de noticias GPIO
    let news_start_minute_gpio = news_start_minute.clone();
    let news_end_minute_gpio = news_end_minute.clone();
    
    // Clones para el hilo principal
    let _station_direction_main = station_direction.clone();
    let _station_switch_main = station_switch.clone();
    let _current_station_main = current_station_index.clone();
    let _saved_station_main = saved_station_index.clone();
    
    let last_station_path_clone = last_station_path.clone();
    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                r_running.store(false, Ordering::SeqCst);
                let index = r_current_index.load(Ordering::SeqCst);
                let _ = save_last_station_index(&last_station_path_clone.to_string_lossy(), index);
            }
            Err(_) => {}
        }
    });
    
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    
    #[cfg(target_os = "linux")]
    {
        let last_button_next_state = Arc::new(AtomicBool::new(false));
        let last_button_prev_state = Arc::new(AtomicBool::new(false));
        let last_button_timer_state = Arc::new(AtomicBool::new(false));
        let timer_press_start = Arc::new(AtomicUsize::new(0));
        
        let r_button_next_state = last_button_next_state.clone();
        let r_button_prev_state = last_button_prev_state.clone();
        let r_button_timer_state = last_button_timer_state.clone();
        let news_button_state = Arc::new(AtomicBool::new(false));
        let r_timer_press_start = timer_press_start.clone();
        let r_station_switch_gpio = station_switch.clone();
        let r_station_direction_gpio = station_direction.clone();
        let r_current_index_gpio = current_station_index.clone();
        let r_timer_minutes_gpio = timer_minutes.clone();
        let r_timer_active_gpio = timer_active.clone();
        let r_timer_should_poweroff_gpio = timer_should_poweroff.clone();
        let r_news_enabled_gpio = news_enabled.clone();
        let r_news_active_gpio = news_active.clone();
        let r_news_station_gpio = news_station.clone();
        let r_news_button_state = news_button_state.clone();
        
        match Gpio::new() {
            Ok(gpio) => {
                if let Ok(pin_next) = gpio.get(GPIO_PIN_NEXT) {
                    let pin_next = pin_next.into_input_pullup();
                    
                    if let Ok(pin_prev) = gpio.get(GPIO_PIN_PREV) {
                        let pin_prev = pin_prev.into_input_pullup();
                        
                        if let Ok(pin_timer) = gpio.get(GPIO_PIN_TIMER) {
                            let pin_timer = pin_timer.into_input_pullup();
                            
                            if let Ok(pin_news) = gpio.get(GPIO_PIN_NEWS) {
                                let pin_news = pin_news.into_input_pullup();
                                
                                tokio::spawn(async move {
                                    while r_running_gpio.load(Ordering::SeqCst) {
                                        let next_current = pin_next.read() == Level::Low;
                                        let next_last = r_button_next_state.load(Ordering::SeqCst);
                                        
                                        if next_current && !next_last {
                                            sleep(Duration::from_millis(DEBOUNCE_MS)).await;
                                            
                                            if pin_next.read() == Level::Low {
                                                let current = r_current_index_gpio.load(Ordering::SeqCst);
                                                let next = (current + 1) % stations_count;
                                                r_current_index_gpio.store(next, Ordering::SeqCst);
                                                r_station_direction_gpio.store(1, Ordering::SeqCst);
                                                r_station_switch_gpio.store(true, Ordering::SeqCst);
                                            }
                                        }
                                        
                                        let prev_current = pin_prev.read() == Level::Low;
                                        let prev_last = r_button_prev_state.load(Ordering::SeqCst);
                                        
                                        if prev_current && !prev_last {
                                            sleep(Duration::from_millis(DEBOUNCE_MS)).await;
                                            
                                            if pin_prev.read() == Level::Low {
                                                let current = r_current_index_gpio.load(Ordering::SeqCst);
                                                let prev = if current == 0 { stations_count - 1 } else { current - 1 };
                                                r_current_index_gpio.store(prev, Ordering::SeqCst);
                                                r_station_direction_gpio.store(-1, Ordering::SeqCst);
                                                r_station_switch_gpio.store(true, Ordering::SeqCst);
                                            }
                                        }
                                        
                                        let timer_current = pin_timer.read() == Level::Low;
                                        let timer_last = r_button_timer_state.load(Ordering::SeqCst);
                                        
                                        if timer_current && !timer_last {
                                            let now = std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap()
                                                .as_secs() as usize;
                                            r_timer_press_start.store(now, Ordering::SeqCst);
                                        }
                                        
                                        if timer_current {
                                            let start_time = r_timer_press_start.load(Ordering::SeqCst);
                                            if start_time != 0 {
                                                let now = std::time::SystemTime::now()
                                                    .duration_since(std::time::UNIX_EPOCH)
                                                    .unwrap()
                                                    .as_secs() as usize;
                                                let press_duration = now.saturating_sub(start_time);
                                                
                                                if press_duration >= (LONG_PRESS_MS / 1000) as usize {
                                                    shutdown_lcd();
                                                    let _ = std::process::Command::new("sudo").arg("poweroff").spawn();
                                                    let _ = std::process::Command::new("sudo").arg("poweroff").output();
                                                    unsafe {
                                                        let cmd = std::ffi::CString::new("sudo poweroff").unwrap();
                                                        libc::system(cmd.as_ptr());
                                                    }
                                                    r_timer_should_poweroff_gpio.store(true, Ordering::SeqCst);
                                                    r_running_gpio.store(false, Ordering::SeqCst);
                                                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                                                    break;
                                                }
                                            }
                                        }
                                        
                                        if !timer_current && timer_last {
                                            let start_time = r_timer_press_start.load(Ordering::SeqCst);
                                            let now = std::time::SystemTime::now()
                                                .duration_since(std::time::UNIX_EPOCH)
                                                .unwrap()
                                                .as_secs() as usize;
                                            let press_duration = now.saturating_sub(start_time);
                                            
                                            if press_duration < (LONG_PRESS_MS / 1000) as usize {
                                                let current_minutes = r_timer_minutes_gpio.load(Ordering::SeqCst);
                                                let new_minutes = if current_minutes == 0 {
                                                    90
                                                } else if current_minutes >= 10 {
                                                    current_minutes - 10
                                                } else {
                                                    0
                                                };
                                                
                                                if new_minutes == 0 {
                                                    r_timer_active_gpio.store(false, Ordering::SeqCst);
                                                    r_timer_minutes_gpio.store(0, Ordering::SeqCst);
                                                    r_timer_start.store(0, Ordering::SeqCst);
                                                } else {
                                                    let now = std::time::SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .unwrap()
                                                        .as_secs() as usize;
                                                    r_timer_start.store(now, Ordering::SeqCst);
                                                    r_timer_minutes_gpio.store(new_minutes, Ordering::SeqCst);
                                                    r_timer_active_gpio.store(true, Ordering::SeqCst);
                                                }
                                            }
                                        }
                                        
                                        let news_current = pin_news.read() == Level::Low;
                                        let news_last = r_news_button_state.load(Ordering::SeqCst);
                                        
                                        if news_current && !news_last {
                                            sleep(Duration::from_millis(DEBOUNCE_MS)).await;
                                            
                                            if pin_news.read() == Level::Low {
                                                let current_state = r_news_enabled_gpio.load(Ordering::SeqCst);
                                                let new_state = !current_state;
                                                r_news_enabled_gpio.store(new_state, Ordering::SeqCst);
                                                
                                                if new_state {
                                                    let _ = load_news_config(&news_minutes_path.to_string_lossy(), &news_start_minute_gpio, &news_end_minute_gpio);
                                                } else {
                                                    r_news_active_gpio.store(false, Ordering::SeqCst);
                                                    r_news_station_gpio.store(false, Ordering::SeqCst);
                                                }
                                            }
                                        }
                                        
                                        r_button_next_state.store(next_current, Ordering::SeqCst);
                                        r_button_prev_state.store(prev_current, Ordering::SeqCst);
                                        r_button_timer_state.store(timer_current, Ordering::SeqCst);
                                        r_news_button_state.store(news_current, Ordering::SeqCst);
                                        sleep(Duration::from_millis(10)).await;
                                    }
                                });
                            }
                        }
                    }
                }
            }
            Err(_) => {}
        }
    }
    
    let timer_running_main = running.clone();
    let timer_minutes_check = timer_minutes.clone();
    let timer_active_check = timer_active.clone();
    let timer_start_check = timer_start_time.clone();
    let timer_poweroff_check = timer_should_poweroff.clone();
    let news_enabled_check = news_enabled.clone();
    let news_active_check = news_active.clone();
    let news_station_check = news_station.clone();
    let news_start_minute_check = news_start_minute.clone();
    let news_end_minute_check = news_end_minute.clone();
    let current_station_index_check = current_station_index.clone();
    let saved_station_index_check = saved_station_index.clone();
    let r_running_main = running.clone();
    
    tokio::spawn(async move {
        let mut last_minute_displayed = usize::MAX;
        
        while timer_running_main.load(Ordering::SeqCst) {
            if timer_poweroff_check.load(Ordering::SeqCst) {
                shutdown_lcd();
                let _ = std::process::Command::new("sudo").arg("poweroff").spawn();
                let _ = std::process::Command::new("sudo").arg("poweroff").output();
                std::process::exit(0);
            }
            
            if timer_active_check.load(Ordering::SeqCst) {
                let minutes = timer_minutes_check.load(Ordering::SeqCst);
                
                if minutes == 0 {
                    if last_minute_displayed != 0 {
                        last_minute_displayed = 0;
                        timer_start_check.store(0, Ordering::SeqCst);
                    }
                } else {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as usize;
                    
                    let start = timer_start_check.load(Ordering::SeqCst);
                    if start == 0 {
                        timer_start_check.store(now, Ordering::SeqCst);
                        last_minute_displayed = minutes;
                    } else {
                        let elapsed = now.saturating_sub(start);
                        let elapsed_minutes = elapsed / 60;
                        
                        if elapsed_minutes >= minutes {
                            timer_running_main.store(false, Ordering::SeqCst);
                            r_running_main.store(false, Ordering::SeqCst);
                            
                            shutdown_lcd();
                            
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            
                            let _ = Command::new("sudo").arg("poweroff").output();
                            std::process::exit(0);
                        } else {
                            let remaining = minutes - elapsed_minutes;
                            if remaining != last_minute_displayed {
                                last_minute_displayed = remaining;
                            }
                        }
                    }
                }
            } else {
                if last_minute_displayed != usize::MAX {
                    last_minute_displayed = usize::MAX;
                }
            }
            
            if news_enabled_check.load(Ordering::SeqCst) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                
                let current_minute = (now % 3600) / 60;
                let start_u64 = news_start_minute_check.load(Ordering::SeqCst) as u64;
                let end_u64 = news_end_minute_check.load(Ordering::SeqCst) as u64;
                
                let should_be_news = if start_u64 <= end_u64 {
                    current_minute >= start_u64 && current_minute < end_u64
                } else {
                    current_minute >= start_u64 || current_minute < end_u64
                };
                
                if should_be_news && !news_station_check.load(Ordering::SeqCst) {
                    news_station_check.store(true, Ordering::SeqCst);
                    news_active_check.store(true, Ordering::SeqCst);
                    
                    let current_index = current_station_index_check.load(Ordering::SeqCst);
                    saved_station_index_check.store(current_index, Ordering::SeqCst);
                } else if !should_be_news && news_station_check.load(Ordering::SeqCst) {
                    news_station_check.store(false, Ordering::SeqCst);
                    news_active_check.store(false, Ordering::SeqCst);
                    
                    let saved_index = saved_station_index_check.load(Ordering::SeqCst);
                    current_station_index_check.store(saved_index, Ordering::SeqCst);
                }
            }
            
            sleep(Duration::from_secs(1)).await;
        }
        
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    });
    
    while running.load(Ordering::SeqCst) {
        let current_index = current_station_index.load(Ordering::SeqCst);
        let direction = station_direction.load(Ordering::SeqCst);
        
        let should_play_news = {
            if news_enabled.load(Ordering::SeqCst) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                
                let current_minute = (now % 3600) / 60;
                let start_u64 = news_start_minute.load(Ordering::SeqCst) as u64;
                let end_u64 = news_end_minute.load(Ordering::SeqCst) as u64;
                
                if start_u64 <= end_u64 {
                    current_minute >= start_u64 && current_minute < end_u64
                } else {
                    current_minute >= start_u64 || current_minute < end_u64
                }
            } else {
                false
            }
        };
        
        let current_station = if should_play_news && !news_url.is_empty() {
            &news_url
        } else {
            &stations[current_index]
        };
        
        let mut child = if unsafe { libc::geteuid() } == 0 {
            Command::new("./run_cvlc.sh")
                .arg(current_station)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?
        } else {
            Command::new("cvlc")
                .arg("--no-video")
                .arg("--no-interact")
                .arg("--quiet")
                .arg(current_station)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?
        };
        
        let pid = child.id();
        
        while running.load(Ordering::SeqCst) && !station_switch.load(Ordering::SeqCst) {
            let current_index = current_station_index.load(Ordering::SeqCst);
            let news_enabled_val = news_enabled.load(Ordering::SeqCst);
            let timer_minutes_val = timer_minutes.load(Ordering::SeqCst);
            let timer_active_val = timer_active.load(Ordering::SeqCst);
            
            let remaining_minutes = if timer_active_val && timer_minutes_val > 0 {
                let start_time = timer_start_time.load(Ordering::SeqCst);
                if start_time > 0 {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as usize;
                    let elapsed_minutes = (now.saturating_sub(start_time)) / 60;
                    let remaining = timer_minutes_val.saturating_sub(elapsed_minutes);
                    if remaining == 0 { 1 } else { remaining }
                } else {
                    timer_minutes_val
                }
            } else {
                0
            };
            
            update_display(
                current_index,
                &station_names,
                news_enabled_val,
                remaining_minutes,
                timer_active_val,
            );
            
            let should_be_news_now = {
                if news_enabled.load(Ordering::SeqCst) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    
                    let current_minute = (now % 3600) / 60;
                    let start_u64 = news_start_minute.load(Ordering::SeqCst) as u64;
                    let end_u64 = news_end_minute.load(Ordering::SeqCst) as u64;
                    
                    if start_u64 <= end_u64 {
                        current_minute >= start_u64 && current_minute < end_u64
                    } else {
                        current_minute >= start_u64 || current_minute < end_u64
                    }
                } else {
                    false
                }
            };
            
            let currently_playing_news = !news_url.is_empty() && 
                *current_station == news_url;
            
            if should_be_news_now != currently_playing_news {
                station_switch.store(true, Ordering::SeqCst);
                break;
            }
            
            sleep(Duration::from_millis(1000)).await;
        }
        
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        
        if unsafe { libc::geteuid() } == 0 {
            let _ = Command::new("pkill")
                .arg("-f")
                .arg("cvlc")
                .output();
        }
        
        match tokio::time::timeout(Duration::from_secs(3), async {
            child.wait()
        }).await {
            Ok(Ok(_)) => {}
            Ok(Err(_)) => {}
            Err(_) => {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
                let _ = child.wait();
            }
        }
        
        station_switch.store(false, Ordering::SeqCst);
        
        if direction != 2 {
            station_direction.store(1, Ordering::SeqCst);
        }
        
        if let Err(e) = save_last_station_index(&last_station_path.to_string_lossy(), current_station_index.load(Ordering::SeqCst)) {
            eprintln!("Error guardando estación: {}", e);
        }
        
        if direction == 3 {
            news_station.store(false, Ordering::SeqCst);
            news_active.store(false, Ordering::SeqCst);
            station_direction.store(1, Ordering::SeqCst);
        }
        
        if !running.load(Ordering::SeqCst) {
            shutdown_lcd();
            break;
        }
    }
    
    shutdown_lcd();
    
    Ok(())
}

fn get_all_stations_from_m3u(filename: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let mut stations = Vec::new();
    
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        
        stations.push(line.to_string());
    }
    
    Ok(stations)
}

fn load_news_config(filename: &str, start_minute: &AtomicUsize, end_minute: &AtomicUsize) -> Result<(), Box<dyn std::error::Error>> {
    match File::open(filename) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let lines: Vec<String> = reader.lines()
                .collect::<Result<Vec<String>, _>>()?
                .into_iter()
                .filter(|line| !line.trim().is_empty())
                .collect();
            
            if lines.len() >= 1 {
                if let Ok(start) = lines[0].trim().parse::<usize>() {
                    start_minute.store(start, Ordering::SeqCst);
                } else {
                    start_minute.store(0, Ordering::SeqCst);
                }
            } else {
                start_minute.store(0, Ordering::SeqCst);
            }
            
            if lines.len() >= 2 {
                if let Ok(end) = lines[1].trim().parse::<usize>() {
                    end_minute.store(end, Ordering::SeqCst);
                } else {
                    end_minute.store(5, Ordering::SeqCst);
                }
            } else {
                end_minute.store(5, Ordering::SeqCst);
            }
        }
        Err(_) => {
            start_minute.store(0, Ordering::SeqCst);
            end_minute.store(5, Ordering::SeqCst);
        }
    }
    
    Ok(())
}

fn get_last_station_index(filename: &str) -> Result<usize, Box<dyn std::error::Error>> {
    if let Some(parent) = Path::new(filename).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    
    match File::open(filename) {
        Ok(file) => {
            let reader = BufReader::new(file);
            if let Some(Ok(line)) = reader.lines().next() {
                match line.trim().parse::<usize>() {
                    Ok(index) => Ok(index),
                    Err(_) => {
                        save_last_station_index(filename, 0)?;
                        Ok(0)
                    }
                }
            } else {
                save_last_station_index(filename, 0)?;
                Ok(0)
            }
        }
        Err(_) => {
            save_last_station_index(filename, 0)?;
            Ok(0)
        }
    }
}

fn save_last_station_index(filename: &str, index: usize) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = Path::new(filename).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    
    let mut file = File::create(filename)?;
    file.write_all(index.to_string().as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    
    Ok(())
}

fn get_station_names_from_m3u(filename: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    
    let mut names = Vec::new();
    
    for line in reader.lines() {
        let line = line?;
        if line.starts_with("#EXTINF:-1,") {
            let name = line[11..].to_string();
            names.push(name);
        }
    }
    
    Ok(names)
}

fn update_lcd_display(
    station_index: usize,
    station_names: &[String],
    news_enabled: bool,
    timer_minutes: usize,
    timer_active: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let station_name = if station_index < station_names.len() {
        &station_names[station_index]
    } else {
        "Desconocida"
    };
    
    let truncated_name = if station_name.len() > 16 {
        format!("{}...", &station_name[..13])
    } else {
        station_name.to_string()
    };
    
    let line1 = format!("{:2}-{}", station_index + 1, truncated_name);
    
    let news_char = if news_enabled { "N" } else { " " };
    
    let timer_str = if timer_active && timer_minutes > 0 {
        format!("{:02}m", timer_minutes)
    } else {
        "   ".to_string()
    };
    
    let time_str = if cfg!(target_os = "linux") {
        match std::process::Command::new("date")
            .arg("+%H:%M:%S")
            .output() {
            Ok(output) => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            Err(_) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let hours = (now / 3600) % 24;
                let minutes = (now / 60) % 60;
                let seconds = now % 60;
                format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
            }
        }
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let hours = (now / 3600) % 24;
        let minutes = (now / 60) % 60;
        let seconds = now % 60;
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    };
    
    let line2 = format!("{} {}   {}", news_char, timer_str, time_str);
    
    let line2 = if line2.len() > 16 {
        &line2[..16]
    } else {
        &line2
    };
    
    send_to_lcd1602_no_clear(&line1, line2)?;
    
    Ok(())
}

fn send_to_lcd1602_no_clear(line1: &str, line2: &str) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        if let Some(ref mut lcd) = GLOBAL_LCD {
            lcd.set_cursor(0, 0)?;
            lcd.write_str(line1)?;
            
            lcd.set_cursor(0, 1)?;
            lcd.write_str(line2)?;
            
            Ok(())
        } else {
            Err("LCD1602 no inicializado".into())
        }
    }
}

struct LCD1602 {
    i2c: I2c,
    addr: u16,
    backlight: bool,
}

impl LCD1602 {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut i2c = I2c::new()?;
        
        let addresses = [0x27, 0x3F, 0x20, 0x38];
        let mut addr_found = None;
        
        for &addr in &addresses {
            if i2c.set_slave_address(addr).is_ok() {
                if i2c.write(&[0]).is_ok() {
                    addr_found = Some(addr);
                    break;
                }
            }
        }
        
        let addr = addr_found.ok_or("No se encontró el LCD en ninguna dirección común")?;
        
        let mut lcd = LCD1602 {
            i2c,
            addr,
            backlight: true,
        };
        
        lcd.init()?;
        Ok(lcd)
    }
    
    fn init(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.i2c.set_slave_address(self.addr)?;
        
        std::thread::sleep(Duration::from_millis(50));
        
        self.write_4bits(0x03 << 4)?;
        std::thread::sleep(Duration::from_micros(4500));
        self.write_4bits(0x03 << 4)?;
        std::thread::sleep(Duration::from_micros(4500));
        self.write_4bits(0x03 << 4)?;
        std::thread::sleep(Duration::from_micros(150));
        self.write_4bits(0x02 << 4)?;
        
        self.command(LCD_FUNCTIONSET | LCD_2LINE | LCD_4BITMODE)?;
        
        self.command(LCD_DISPLAYCONTROL | LCD_DISPLAYON | LCD_CURSOROFF | LCD_BLINKOFF)?;
        
        self.clear()?;
        
        self.command(LCD_ENTRYMODESET | LCD_ENTRYLEFT | LCD_ENTRYSHIFTDECREMENT)?;
        
        std::thread::sleep(Duration::from_millis(100));
        
        Ok(())
    }
    
    fn write_4bits(&mut self, data: u8) -> Result<(), Box<dyn std::error::Error>> {
        let mut value = data;
        if self.backlight {
            value |= LCD_BACKLIGHT;
        }
        
        self.i2c.write(&[value | LCD_ENABLE])?;
        std::thread::sleep(Duration::from_micros(1));
        self.i2c.write(&[value & !LCD_ENABLE])?;
        std::thread::sleep(Duration::from_micros(50));
        
        Ok(())
    }
    
    fn command(&mut self, cmd: u8) -> Result<(), Box<dyn std::error::Error>> {
        self.send(cmd, 0)
    }
    
    fn send(&mut self, data: u8, mode: u8) -> Result<(), Box<dyn std::error::Error>> {
        let high = mode | (data & 0xF0);
        let low = mode | ((data << 4) & 0xF0);
        
        self.write_4bits(high)?;
        self.write_4bits(low)?;
        
        Ok(())
    }
    
    fn clear(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.command(LCD_CLEARDISPLAY)?;
        std::thread::sleep(Duration::from_millis(2));
        Ok(())
    }
    
    fn set_cursor(&mut self, col: u8, row: u8) -> Result<(), Box<dyn std::error::Error>> {
        let row_offsets = [0x00, 0x40, 0x14, 0x54];
        let row = if row > 3 { 3 } else { row };
        self.command(LCD_SETDDRAMADDR | (col + row_offsets[row as usize]))?;
        Ok(())
    }
    
    fn write_char(&mut self, c: char) -> Result<(), Box<dyn std::error::Error>> {
        self.send(c as u8, LCD_RS)?;
        Ok(())
    }
    
    fn write_str(&mut self, s: &str) -> Result<(), Box<dyn std::error::Error>> {
        for c in s.chars() {
            self.write_char(c)?;
        }
        Ok(())
    }
    
    fn set_backlight(&mut self, state: bool) {
        self.backlight = state;
    }
}

static mut GLOBAL_LCD: Option<LCD1602> = None;

fn shutdown_lcd() {
    unsafe {
        if let Some(ref mut lcd) = GLOBAL_LCD {
            let _ = lcd.clear();
            std::thread::sleep(std::time::Duration::from_millis(100));
            lcd.set_backlight(false);
            std::thread::sleep(std::time::Duration::from_millis(100));
            let _ = lcd.command(0x08);
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
}

fn init_lcd1602() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        match LCD1602::new() {
            Ok(lcd) => {
                GLOBAL_LCD = Some(lcd);
                Ok(())
            },
            Err(e) => Err(e)
        }
    }
}

fn update_display(
    station_index: usize,
    station_names: &[String],
    news_enabled: bool,
    timer_minutes: usize,
    timer_active: bool,
) {
    let station_name = if station_index < station_names.len() {
        &station_names[station_index]
    } else {
        "Desconocida"
    };
    
    let _line1 = format!("{:2}-{}", station_index + 1, station_name);
    
    let news_char = if news_enabled { "N" } else { " " };
    
    let timer_str = if timer_active && timer_minutes > 0 {
        format!("{:02}m", timer_minutes)
    } else {
        "   ".to_string()
    };
    
    let time_str = if cfg!(target_os = "linux") {
        match std::process::Command::new("date")
            .arg("+%H:%M:%S")
            .output() {
            Ok(output) => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            Err(_) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let hours = (now / 3600) % 24;
                let minutes = (now % 3600) / 60;
                let seconds = now % 60;
                format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
            }
        }
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let hours = (now / 3600) % 24;
        let minutes = (now % 3600) / 60;
        let seconds = now % 60;
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    };
    
    let _line2 = format!("{} {}   {}", news_char, timer_str, time_str);
    
    let _ = update_lcd_display(station_index, station_names, news_enabled, timer_minutes, timer_active);
}
