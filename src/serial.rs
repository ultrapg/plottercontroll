use serialport::SerialPort;
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;

/// Status updates from the serial streaming thread.
#[derive(Debug, Clone)]
pub enum SerialEvent {
    /// A line was sent and acknowledged.
    LineSent { line: String, line_number: u32, total: u32 },
    /// An error occurred.
    Error(String),
    /// Streaming is complete.
    Complete,
    /// Connection opened successfully.
    Connected,
    /// Connection closed.
    Disconnected,
    /// Raw response from printer.
    Response(String),
}

/// Open a serial port to the printer.
pub fn open_port(port_name: &str, baud_rate: u32) -> anyhow::Result<Box<dyn SerialPort>> {
    let port = serialport::new(port_name, baud_rate)
        .timeout(Duration::from_millis(5000))
        .open()?;
    log::info!("Opened serial port {} at {} baud", port_name, baud_rate);
    Ok(port)
}

/// List available serial ports.
pub fn list_ports() -> Vec<String> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.port_name)
        .collect()
}

/// Stream G-code to the printer with Marlin ok handshake.
/// Runs in a background thread. Returns a receiver for status events.
/// The cancel flag allows the caller to abort streaming.
pub fn stream_gcode(
    mut port: Box<dyn SerialPort>,
    gcode: &str,
    cancel: Arc<AtomicBool>,
) -> Receiver<SerialEvent> {
    let (tx, rx) = mpsc::channel();

    let gcode = gcode.to_string();
    thread::spawn(move || {
        let _ = tx.send(SerialEvent::Connected);

        let lines: Vec<&str> = gcode.lines().collect();
        let total = lines.len() as u32;
        let mut line_number = 0u32;
        let mut reader = BufReader::new(port.try_clone().unwrap());

        for line in lines {
            if cancel.load(Ordering::Relaxed) {
                let _ = tx.send(SerialEvent::Disconnected);
                return;
            }

            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with(';') {
                line_number += 1;
                let _ = tx.send(SerialEvent::LineSent {
                    line: line.to_string(),
                    line_number,
                    total,
                });
                continue;
            }

            // Send the command
            let cmd = format!("{}\r\n", trimmed);
            if let Err(e) = port.write_all(cmd.as_bytes()) {
                let _ = tx.send(SerialEvent::Error(format!("Write error: {}", e)));
                break;
            }
            if let Err(e) = port.flush() {
                let _ = tx.send(SerialEvent::Error(format!("Flush error: {}", e)));
                break;
            }

            // Wait for 'ok' acknowledgment
            let mut ack = String::new();
            loop {
                ack.clear();
                match reader.read_line(&mut ack) {
                    Ok(0) => {
                        // EOF / disconnected
                        let _ = tx.send(SerialEvent::Error("Connection lost".to_string()));
                        let _ = tx.send(SerialEvent::Disconnected);
                        return;
                    }
                    Ok(_) => {
                        let response = ack.trim().to_string();
                        if !response.is_empty() {
                            let _ = tx.send(SerialEvent::Response(response.clone()));
                        }
                        if response == "ok" || response.starts_with("ok ") {
                            break;
                        }
                        // Some Marlin firmware sends 'wait' or other status
                        if response.to_lowercase().contains("error") {
                            log::warn!("Marlin error: {}", response);
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::TimedOut {
                            // Timeout - resend or break
                            let _ = tx.send(SerialEvent::Error(
                                "Timeout waiting for ok".to_string(),
                            ));
                            break;
                        }
                        let _ = tx.send(SerialEvent::Error(format!(
                            "Read error: {}",
                            e
                        )));
                        let _ = tx.send(SerialEvent::Disconnected);
                        return;
                    }
                }
            }

            line_number += 1;
            let _ = tx.send(SerialEvent::LineSent {
                line: trimmed.to_string(),
                line_number,
                total,
            });
        }

        let _ = tx.send(SerialEvent::Complete);
    });

    rx
}

/// Send a single G-code command and wait for ok.
pub fn send_command(port: &mut Box<dyn SerialPort>, command: &str) -> anyhow::Result<String> {
    let cmd = format!("{}\r\n", command);
    port.write_all(cmd.as_bytes())?;
    port.flush()?;

    let mut reader = BufReader::new(port.as_mut());
    let mut response = String::new();
    loop {
        response.clear();
        let n = reader.read_line(&mut response)?;
        if n == 0 {
            anyhow::bail!("Connection closed");
        }
        let trimmed = response.trim().to_string();
        if trimmed == "ok" || trimmed.starts_with("ok ") {
            return Ok(trimmed);
        }
    }
}

/// Send an immediate command (like emergency stop) without waiting.
pub fn send_immediate(port: &mut Box<dyn SerialPort>, command: &str) -> anyhow::Result<()> {
    let cmd = format!("{}\r\n", command);
    port.write_all(cmd.as_bytes())?;
    port.flush()?;
    Ok(())
}
