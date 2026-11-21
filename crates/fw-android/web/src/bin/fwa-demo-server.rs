//! A stand-in compositor on TCP for testing the browser path end to end (page, `fwa-ws-relay`, wire protocol)
//! without the Android guest: it serves the same synthetic windows as the viewer's `?demo=1`.
//!
//! `fwa-demo-server <listen host:port>`

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fw_android_stream::Message;
use fw_android_web::demo::Demo;

const HEADER: usize = 9;

fn read_message(stream: &mut TcpStream) -> std::io::Result<Message> {
    let mut header = [0u8; HEADER];
    stream.read_exact(&mut header)?;
    let length = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
    let mut bytes = header.to_vec();
    bytes.resize(HEADER + length, 0);
    stream.read_exact(&mut bytes[HEADER..])?;
    Message::decode(&bytes).map(|(message, _)| message).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn serve(stream: TcpStream) -> std::io::Result<()> {
    stream.set_nodelay(true)?;
    let demo = Arc::new(Mutex::new(Demo::new()));
    let writer = Arc::new(Mutex::new(stream.try_clone()?));
    let send = |writer: &Mutex<TcpStream>, messages: Vec<Message>| -> std::io::Result<()> {
        let mut stream = writer.lock().map_err(|error| std::io::Error::other(error.to_string()))?;
        for message in messages {
            stream.write_all(&message.encode())?;
        }
        Ok(())
    };
    let opening = demo.lock().map_err(|error| std::io::Error::other(error.to_string()))?.open();
    send(&writer, opening)?;

    let reader = {
        let (demo, writer) = (demo.clone(), writer.clone());
        let mut stream = stream;
        std::thread::spawn(move || -> std::io::Result<()> {
            loop {
                let message = read_message(&mut stream)?;
                let replies = demo.lock().map_err(|error| std::io::Error::other(error.to_string()))?.input(&message);
                send(&writer, replies)?;
            }
        })
    };
    let started = Instant::now();
    while !reader.is_finished() {
        std::thread::sleep(Duration::from_millis(16));
        let frames = demo.lock().map_err(|error| std::io::Error::other(error.to_string()))?.step(started.elapsed().as_secs_f64() * 1000.0);
        send(&writer, frames)?;
    }
    reader.join().map_err(|_| std::io::Error::other("reader thread panicked"))?
}

fn main() -> ExitCode {
    let Some(address) = std::env::args().nth(1) else {
        eprintln!("usage: fwa-demo-server <listen host:port>");
        return ExitCode::from(2);
    };
    let listener = match TcpListener::bind(&address) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("fwa-demo-server: bind {address}: {error}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!("fwa-demo-server: tcp://{address}");
    for stream in listener.incoming().flatten() {
        std::thread::spawn(|| {
            if let Err(error) = serve(stream) {
                eprintln!("fwa-demo-server: connection ended: {error}");
            }
        });
    }
    ExitCode::SUCCESS
}
